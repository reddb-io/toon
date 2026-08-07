/**
 * Builds a JSON value tree from the decode event stream — the whole-document
 * convenience over `decodeFromLines` (ADR 0006). Mirrors upstream's
 * event-builder layering: the stream is the core, the tree is derived.
 */

import type { ToonEvent } from '../events.js'
import { decodeFromLines } from './stream.js'
import type { DecodeStreamOptions } from './stream.js'
import { expandCyclicDiscriminatedArrays } from '../toon_parts/cyclic.js'
import { parse as parseLegacyExtensions } from '../toon_parts/parse.js'

export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue }

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
      // duplicate keys are last-write-wins (§14.3); defineProperty keeps
      // prototype keys like __proto__ ordinary own keys (§15)
      Object.defineProperty(parent, pendingKey as string, {
        value,
        enumerable: true,
        writable: true,
        configurable: true,
      })
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

export function decodeValue(input: string, options?: DecodeStreamOptions): JsonValue {
  let value: JsonValue
  if (
    hasPrimitiveArrayColumnHeader(input) ||
    (options?.objectArrayColumns !== false && hasFixedArrayColumnHeader(input))
  ) {
    value = parseLegacyExtensions(input, {
      indent: options?.indentSize ?? options?.indent,
      strict: options?.strict,
      cyclicDiscriminatedArrays: false,
      maxDepth: options?.maxDepth,
    }) as JsonValue
  } else {
    try {
      value = buildValueFromEvents(decodeFromLines(linesFromString(input), options))
    } catch (error) {
      if (options?.objectArrayColumns === false || !hasChildTableHeader(input)) throw error
      value = parseLegacyExtensions(input, {
        indent: options?.indentSize ?? options?.indent,
        strict: options?.strict,
        cyclicDiscriminatedArrays: false,
        maxDepth: options?.maxDepth,
      }) as JsonValue
    }
  }
  return (options?.cyclicDiscriminatedArrays === true
    ? expandCyclicDiscriminatedArrays(value)
    : value) as JsonValue
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

function hasPrimitiveArrayColumnHeader(input: string): boolean {
  return headerFieldLists(input).some((fields) =>
    [...fields.matchAll(/\[([^\]]*)\]/g)].some(
      ([, content]) => !/^(?:0|[1-9]\d*)(?:\t|\|)?$/.test(content),
    ),
  )
}

function hasFixedArrayColumnHeader(input: string): boolean {
  return headerFieldLists(input).some((fields) =>
    /\[(?:0|[1-9]\d*)(?:\t|\|)?\]/.test(fields),
  )
}

function headerFieldLists(input: string): string[] {
  return input.split(/\r?\n/).flatMap((line) => {
    const outerClose = line.indexOf(']')
    if (outerClose === -1) return []
    const fieldsStart = line.indexOf('{', outerClose + 1)
    if (fieldsStart === -1) return []
    const fieldsEnd = line.lastIndexOf('}')
    return fieldsEnd > fieldsStart ? [line.slice(fieldsStart + 1, fieldsEnd)] : []
  })
}

function hasChildTableHeader(input: string): boolean {
  return input.split(/\r?\n/).some((line) => {
    const fieldsStart = line.indexOf('{', line.indexOf(']') + 1)
    const fieldsEnd = line.lastIndexOf('}')
    if (fieldsStart === -1 || fieldsEnd <= fieldsStart) return false
    const fields = line.slice(fieldsStart + 1, fieldsEnd)
    return fields.includes('{') || /\[(?:0|[1-9]\d*)(?:\t|\|)?\]/.test(fields)
  })
}
