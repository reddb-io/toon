/**
 * Event-based streaming decoder (ADR 0006).
 *
 * Consumes an iterable of TOON lines (without newlines) and yields the six
 * JSON-semantic events, each carrying its 1-based source line. Errors are
 * fail-fast positioned `ToonError`s; strict-mode policy is resolved here at
 * the public boundary — the helpers below stay policy-free.
 */

import type { ToonEvent } from '../events.js'
import { ToonError, toonError } from '../errors.js'
import { findUnquoted, parseKey, parseScalar, splitDelimited } from '../lexical.js'

export interface DecodeStreamOptions {
  indent?: number
  strict?: boolean
}

interface Ctx {
  indentSize: number
  strict: boolean
}

interface Line {
  number: number
  depth: number
  content: string
}

const COMMENT_MARKER = '#'

/** A full-line comment: only U+0020 spaces before `#` (v4.1 §5.1). */
function isCommentLine(raw: string): boolean {
  let i = 0
  while (i < raw.length && raw[i] === ' ') i++
  return raw[i] === COMMENT_MARKER
}

function classifyLines(source: Iterable<string>, ctx: Ctx): Line[] {
  const lines: Line[] = []
  let number = 0
  for (const raw of source) {
    number++
    if (raw.trim() === '') continue
    if (isCommentLine(raw)) continue
    let spaces = 0
    while (spaces < raw.length && raw[spaces] === ' ') spaces++
    if (spaces % ctx.indentSize !== 0) {
      throw toonError(number, 'invalid indentation')
    }
    lines.push({ number, depth: spaces / ctx.indentSize, content: raw.slice(spaces) })
  }
  return lines
}

interface Header {
  key: string | undefined
  length: number
  fields: string[] | undefined
  inline: string | undefined
}

/** Parses `key[N]{a,b}:` / `key[N]: v1,v2` array-header content, or null. */
function parseArrayHeader(content: string, line: number): Header | null {
  const bracket = findUnquoted(content, '[', line)
  if (bracket === -1) return null
  const colon = findUnquoted(content, ':', line)
  if (colon !== -1 && colon < bracket) return null
  const close = content.indexOf(']', bracket)
  if (close === -1) return null
  const lengthText = content.slice(bracket + 1, close)
  if (!/^\d+$/.test(lengthText)) {
    throw toonError(line, 'malformed array header length')
  }
  const length = Number(lengthText)
  let rest = content.slice(close + 1)
  let fields: string[] | undefined
  if (rest.startsWith('{')) {
    const endBrace = rest.indexOf('}')
    if (endBrace === -1) throw toonError(line, 'malformed tabular header fields')
    fields = splitDelimited(rest.slice(1, endBrace), ',', line).map(
      (field: string) => parseKey(field.trim(), line)[0] as string,
    )
    rest = rest.slice(endBrace + 1)
  }
  if (!rest.startsWith(':')) throw toonError(line, 'expected colon after array header')
  const after = rest.slice(1).trim()
  const key =
    bracket === 0 ? undefined : (parseKey(content.slice(0, bracket).trim(), line)[0] as string)
  return { key, length, fields, inline: after === '' ? undefined : after }
}

class Reader {
  private index = 0
  constructor(private readonly lines: Line[]) {}
  peek(): Line | undefined {
    return this.lines[this.index]
  }
  next(): Line | undefined {
    return this.lines[this.index++]
  }
  /** The line number where the previous block "ends": the last consumed line. */
  lastNumber(fallback: number): number {
    const previous = this.lines[this.index - 1]
    return previous === undefined ? fallback : previous.number
  }
}

export function* decodeStreamSync(
  source: Iterable<string>,
  options?: DecodeStreamOptions,
): Generator<ToonEvent> {
  const ctx: Ctx = { indentSize: options?.indent ?? 2, strict: options?.strict ?? true }
  const lines = classifyLines(source, ctx)
  const reader = new Reader(lines)

  const first = reader.peek()
  if (first !== undefined && first.depth !== 0) {
    throw toonError(first.number, 'invalid indentation')
  }

  yield* emitObject(reader, 0, first?.number ?? 1, ctx)

  const trailing = reader.peek()
  if (trailing !== undefined) {
    throw toonError(trailing.number, 'expected end of document')
  }
}

function* emitObject(
  reader: Reader,
  depth: number,
  startLine: number,
  ctx: Ctx,
): Generator<ToonEvent> {
  yield { type: 'startObject', line: startLine }
  while (true) {
    const line = reader.peek()
    if (line === undefined || line.depth < depth) break
    if (line.depth > depth) throw toonError(line.number, 'invalid indentation')
    reader.next()
    yield* emitEntry(reader, line, ctx)
  }
  yield { type: 'endObject', line: reader.lastNumber(startLine) }
}

function* emitEntry(reader: Reader, line: Line, ctx: Ctx): Generator<ToonEvent> {
  const header = parseArrayHeader(line.content, line.number)
  if (header !== undefined && header !== null) {
    if (header.key === undefined) {
      throw toonError(line.number, 'root array form is not valid inside an object')
    }
    yield { type: 'key', key: header.key, line: line.number }
    yield* emitArray(reader, line, header, ctx)
    return
  }

  const colon = findUnquoted(line.content, ':', line.number)
  if (colon === -1) throw toonError(line.number, 'expected key-value line')
  const key = parseKey(line.content.slice(0, colon).trim(), line.number)[0] as string
  const rest = line.content.slice(colon + 1)
  yield { type: 'key', key, line: line.number }

  if (rest.trim() === '') {
    const child = reader.peek()
    if (child !== undefined && child.depth === line.depth + 1) {
      yield* emitObject(reader, line.depth + 1, child.number, ctx)
    } else {
      // `key:` with no nested block is the empty string scalar (v4.1 §7).
      yield { type: 'primitive', value: '', line: line.number }
    }
    return
  }
  yield { type: 'primitive', value: parseScalar(rest.trim(), line.number), line: line.number }
}

function* emitArray(reader: Reader, header: Line, info: Header, ctx: Ctx): Generator<ToonEvent> {
  yield { type: 'startArray', length: info.length, line: header.number }

  if (info.inline !== undefined) {
    const values = splitDelimited(info.inline, ',', header.number)
    assertCount(values.length, info.length, header.number, 'inline-form values', ctx)
    for (const value of values) {
      yield { type: 'primitive', value: parseScalar(value.trim(), header.number), line: header.number }
    }
    yield { type: 'endArray', line: header.number }
    return
  }

  let rows = 0
  while (true) {
    const line = reader.peek()
    if (line === undefined || line.depth <= header.depth) break
    if (line.depth !== header.depth + 1) throw toonError(line.number, 'invalid indentation')
    reader.next()
    rows++

    if (info.fields !== undefined) {
      const cells = splitDelimited(line.content, ',', line.number)
      assertCount(cells.length, info.fields.length, line.number, 'row cells', ctx)
      yield { type: 'startObject', line: line.number }
      for (let i = 0; i < info.fields.length; i++) {
        yield { type: 'key', key: info.fields[i], line: line.number }
        yield { type: 'primitive', value: parseScalar(cells[i].trim(), line.number), line: line.number }
      }
      yield { type: 'endObject', line: line.number }
    } else if (line.content.startsWith('- ')) {
      yield {
        type: 'primitive',
        value: parseScalar(line.content.slice(2).trim(), line.number),
        line: line.number,
      }
    } else {
      throw toonError(line.number, 'expected a list item line')
    }
  }

  const endLine = reader.lastNumber(header.number)
  if (ctx.strict && rows !== info.length) {
    throw toonError(endLine, `expected ${info.length} rows, but got ${rows}`)
  }
  yield { type: 'endArray', line: endLine }
}

function assertCount(got: number, expected: number, line: number, what: string, ctx: Ctx): void {
  if (ctx.strict && got !== expected) {
    throw toonError(line, `expected ${expected} ${what}, but got ${got}`)
  }
}

export { ToonError }
