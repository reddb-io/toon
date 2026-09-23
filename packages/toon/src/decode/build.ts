/**
 * Builds a JSON value tree from the decode event stream — the whole-document
 * convenience over `decodeFromLines` (ADR 0006). Mirrors upstream's
 * event-builder layering: the stream is the core, the tree is derived.
 */

import type { ToonEvent } from '../events.js'
import { decodeFromLines as decodeEventsFromLines } from './stream.js'
import type { DecodeStreamOptions } from './stream.js'
import { expandCyclicDiscriminatedArrays } from '../cyclic.js'
import type { DecodeOptions, JsonValue } from '../types.js'
import { applyReviver } from './reviver.js'
import { ToonDecodeError } from '../errors.js'
import { setKey } from '../lexical.js'

const UNSET = Symbol('unset')

export function buildValueFromEvents(events: Iterable<ToonEvent>): JsonValue {
  const stack: JsonValue[] = []
  let pendingKey: string | undefined
  let root: JsonValue | typeof UNSET = UNSET

  const attach = (value: JsonValue): void => {
    const parent = stack[stack.length - 1]
    if (parent === undefined) {
      root = value
    } else if (Array.isArray(parent)) {
      parent.push(value)
    } else {
      // duplicate keys are last-write-wins (§14.3); setKey keeps prototype
      // keys like __proto__ ordinary own keys (§15)
      setKey(parent, pendingKey as string, value)
    }
  }

  for (const event of events) {
    switch (event.type) {
      case 'startObject': {
        const value: JsonValue = {}
        attach(value)
        stack.push(value)
        break
      }
      case 'startArray': {
        const value: JsonValue[] = []
        attach(value)
        stack.push(value)
        break
      }
      case 'endObject':
      case 'endArray':
        stack.pop()
        break
      case 'key':
        pendingKey = event.key
        break
      case 'primitive':
        attach(event.value)
        break
    }
  }
  return root === UNSET ? {} : root
}

/** Decodes pre-split TOON lines into one JSON value. */
export function decodeFromLines(
  lines: Iterable<string>,
  options?: DecodeOptions,
): JsonValue {
  const value = buildValueFromEvents(decodeEventsFromLines(lines, options) as Iterable<ToonEvent>)
  return options?.reviver ? applyReviver(value, options.reviver) : value
}

export function decodeValue(input: string, options?: DecodeOptions): JsonValue {
  const { reviver, ...streamOptions } = options ?? {}
  // A UTF-16 unit never encodes to fewer UTF-8 bytes, so an oversized string
  // fails here without being split; the line classifier counts the rest.
  const maxInputBytes = options?.maxInputBytes ?? 0
  if (maxInputBytes > 0 && maxInputBytes !== Number.POSITIVE_INFINITY && input.length > maxInputBytes) {
    throw new ToonDecodeError(`input exceeds maxInputBytes (${Math.floor(maxInputBytes)})`, { line: 0 })
  }
  const value = decodeFromLines(linesFromString(input), streamOptions)
  const decoded = (options?.cyclicDiscriminatedArrays === true
    ? expandCyclicDiscriminatedArrays(value)
    : value) as JsonValue
  return reviver ? applyReviver(decoded, reviver) : decoded
}

/** Iterates string lines without allocating a whole-document line array. */
function* linesFromString(input: string): Generator<string> {
  let start = 0
  while (true) {
    const end = input.indexOf('\n', start)
    if (end === -1) {
      yield input.slice(start)
      return
    }
    yield input.slice(start, end)
    start = end + 1
  }
}
