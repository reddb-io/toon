/**
 * TOONL v0.1 — the append-only, line-oriented streaming profile of TOON.
 * Semantics follow `docs/toonl-v0.1.md`: a stream is a sequence of segments,
 * each opened by a `[<delim?>]{fields}:` header, filled with one row per line,
 * and optionally closed by a `[=N]` trailer that asserts the row count.
 */

import { asToonlError, toonlError } from './errors.js'
import { parse, serialize } from './toon.js'
import {
  DOCUMENT_DELIMITER,
  canonicalKey,
  isPrimitive,
  parseKey,
  parseScalar,
  primitiveText,
  setKey,
  splitDelimited,
  splitLines,
} from './lexical.js'

const DELIMITERS = [DOCUMENT_DELIMITER, '|', '\t']

function requireTransformStream() {
  if (typeof TransformStream === 'undefined') {
    throw toonlError(0, 'Web Streams API is not available in this runtime')
  }
  return TransformStream
}

// ---------------------------------------------------------------------------
// Line grammar
// ---------------------------------------------------------------------------

/** `[=N]` → N; `null` when the line is not a trailer. */
function trailerCount(line, lineNumber) {
  if (!(line.startsWith('[=') && line.endsWith(']'))) {
    return null
  }
  const digits = line.slice(2, -1)
  if (!/^[0-9]+$/.test(digits)) {
    throw toonlError(lineNumber, 'invalid trailer count')
  }
  return Number.parseInt(digits, 10)
}

/** `[<delim?>]{fields}:` → `{delimiter, fields}`; `null` when not a header. */
function parseHeaderLine(line, lineNumber) {
  if (!line.startsWith('[')) {
    return null
  }
  const rest = line.slice(1)
  const close = rest.indexOf(']')
  if (close === -1) {
    throw toonlError(lineNumber, 'invalid header')
  }

  const bracket = rest.slice(0, close)
  if (bracket.startsWith('=')) {
    return null
  }
  if (!DELIMITERS.includes(bracket) && bracket !== '') {
    throw toonlError(lineNumber, 'invalid header delimiter')
  }
  const delimiter = bracket === '' ? DOCUMENT_DELIMITER : bracket

  const suffix = rest.slice(close + 1)
  if (!suffix.startsWith('{') || !suffix.endsWith('}:')) {
    throw toonlError(lineNumber, 'invalid header')
  }

  let fields
  try {
    fields = splitDelimited(suffix.slice(1, -2), delimiter, lineNumber).map(
      (field) => parseKey(field, lineNumber)[0],
    )
  } catch (error) {
    throw asToonlError(error)
  }
  if (fields.length === 0 || fields.some((field) => field === '')) {
    throw toonlError(lineNumber, 'invalid header fields')
  }

  return { delimiter, fields }
}

/** Splits a row into raw (still-encoded) cells, validating arity and each scalar. */
function parseRow(line, delimiter, expectedCells, lineNumber) {
  let cells
  try {
    cells = splitDelimited(line, delimiter, lineNumber)
  } catch (error) {
    throw asToonlError(error)
  }
  if (cells.length !== expectedCells) {
    throw toonlError(lineNumber, 'row arity mismatch')
  }
  for (const cell of cells) {
    try {
      parseScalar(cell, lineNumber)
    } catch (error) {
      throw asToonlError(error)
    }
  }
  return cells
}

/** Decodes a raw row into a record keyed by the segment's fields. */
function rowRecord(fields, cells, lineNumber) {
  const record = {}
  fields.forEach((field, index) => {
    try {
      setKey(record, field, parseScalar(cells[index], lineNumber))
    } catch (error) {
      throw asToonlError(error)
    }
  })
  return record
}

/**
 * Classifies one non-blank line against the open segment. Returns the segment
 * transition so both the buffered and the streaming decoder share one grammar.
 */
function classifyLine(line, lineNumber, open) {
  if (line.startsWith('- ')) {
    throw toonlError(lineNumber, 'reserved line prefix')
  }

  const count = trailerCount(line, lineNumber)
  if (count !== null) {
    if (open === null) {
      throw toonlError(lineNumber, 'trailer without header')
    }
    if (open.rowCount !== count) {
      throw toonlError(lineNumber, 'trailer count mismatch')
    }
    return { kind: 'trailer' }
  }

  const header = parseHeaderLine(line, lineNumber)
  if (header !== null) {
    return { kind: 'header', header }
  }

  if (open === null) {
    throw toonlError(lineNumber, 'row before header')
  }
  return {
    kind: 'row',
    cells: parseRow(line, open.delimiter, open.fields.length, lineNumber),
  }
}

// ---------------------------------------------------------------------------
// Buffered decoding
// ---------------------------------------------------------------------------

/**
 * Parses a whole TOONL stream into its segments, each keeping its raw cells.
 * A segment left open at EOF is still returned — TOONL is append-only, and an
 * unterminated tail is the normal shape of a stream someone is still writing.
 */
export function parseStream(input) {
  const segments = []
  let current = null

  splitLines(input).forEach((line, index) => {
    const lineNumber = index + 1
    if (line === '') {
      return
    }

    const open =
      current === null
        ? null
        : { delimiter: current.delimiter, fields: current.fields, rowCount: current.rows.length }
    const step = classifyLine(line, lineNumber, open)

    if (step.kind === 'trailer') {
      segments.push(current)
      current = null
      return
    }
    if (step.kind === 'header') {
      if (current !== null) {
        segments.push(current)
      }
      current = { delimiter: step.header.delimiter, fields: step.header.fields, rows: [] }
      return
    }
    current.rows.push(step.cells)
  })

  if (current !== null) {
    segments.push(current)
  }

  return segments
}

/** Every row of every segment, decoded into records. */
export function parseRecords(input) {
  return parseStream(input).flatMap((segment) =>
    segment.rows.map((row) => rowRecord(segment.fields, row, 0)),
  )
}

/**
 * Closes the stream: each segment becomes one canonical TOON document, so a
 * length-free append-only stream turns into length-bearing TOON (§ close
 * transform). Cells are already canonical, so they are re-emitted verbatim.
 */
export function closeTransform(input) {
  return parseStream(input).map((segment) => {
    const delimiter = segment.delimiter
    const bracket = delimiter === DOCUMENT_DELIMITER ? '' : delimiter
    const fields = segment.fields.map(canonicalKey).join(delimiter)
    const rows = segment.rows.map((row) => `  ${row.join(delimiter)}\n`).join('')
    return `[${segment.rows.length}${bracket}]{${fields}}:\n${rows}`
  })
}

// ---------------------------------------------------------------------------
// Streaming decode
// ---------------------------------------------------------------------------

async function* toLines(source) {
  if (typeof source === 'string') {
    yield* splitLines(source)
    return
  }

  // Chunks may split anywhere, so hold a partial line back until its newline
  // arrives. Whatever remains at EOF is a final line without a trailing newline.
  let buffer = ''
  for await (const chunk of source) {
    buffer += typeof chunk === 'string' ? chunk : new TextDecoder().decode(chunk)
    let newline = buffer.indexOf('\n')
    while (newline !== -1) {
      const line = buffer.slice(0, newline)
      yield line.endsWith('\r') ? line.slice(0, -1) : line
      buffer = buffer.slice(newline + 1)
      newline = buffer.indexOf('\n')
    }
  }
  if (buffer !== '') {
    yield buffer.endsWith('\r') ? buffer.slice(0, -1) : buffer
  }
}

/**
 * Decodes a TOONL stream record by record, without ever holding the stream in
 * memory. Schema rotation is followed automatically, blank lines are skipped,
 * and each `[=N]` trailer is checked against the rows actually seen.
 *
 * `source` is a string, or an (async) iterable of string/Uint8Array chunks.
 */
export async function* decodeLines(source) {
  const state = createDecodeState()

  for await (const line of toLines(source)) {
    const record = consumeDecodeLine(state, line)
    if (record !== undefined) {
      yield record
    }
  }
}

function createDecodeState() {
  return { open: null, lineNumber: 0 }
}

function consumeDecodeLine(state, line) {
  state.lineNumber += 1
  if (line === '') {
    return undefined
  }

  const step = classifyLine(line, state.lineNumber, state.open)
  if (step.kind === 'trailer') {
    state.open = null
    return undefined
  }
  if (step.kind === 'header') {
    state.open = { delimiter: step.header.delimiter, fields: step.header.fields, rowCount: 0 }
    return undefined
  }

  state.open.rowCount += 1
  return rowRecord(state.open.fields, step.cells, state.lineNumber)
}

function chunkText(decoder, chunk, stream) {
  return typeof chunk === 'string' ? chunk : decoder.decode(chunk, { stream })
}

function consumeBufferedLines(state, text, controller, onLine) {
  state.buffer += text
  let newline = state.buffer.indexOf('\n')
  while (newline !== -1) {
    const rawLine = state.buffer.slice(0, newline)
    onLine(state, rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine, controller)
    state.buffer = state.buffer.slice(newline + 1)
    newline = state.buffer.indexOf('\n')
  }
}

function consumeBufferedFlush(state, controller, onLine) {
  if (state.buffer !== '') {
    const line = state.buffer.endsWith('\r') ? state.buffer.slice(0, -1) : state.buffer
    state.buffer = ''
    onLine(state, line, controller)
  }
}

function enqueueRecordJson(record, controller) {
  controller.enqueue(`${JSON.stringify(record)}\n`)
}

function enqueueDecodedRecord(state, line, controller) {
  const record = consumeDecodeLine(state.decoder, line)
  if (record !== undefined) {
    controller.enqueue(record)
  }
}

function enqueueJsonlRecord(state, line, controller) {
  const record = consumeDecodeLine(state.decoder, line)
  if (record !== undefined) {
    enqueueRecordJson(record, controller)
  }
}

function enqueueEncodedJson(state, line, controller) {
  if (line === '') {
    return
  }
  const parsed = JSON.parse(line)
  const output = state.emitter.push(parsed)
  if (output !== '') {
    controller.enqueue(output)
  }
}

function finishEmitter(state, controller) {
  const output = state.emitter.end()
  if (output !== '') {
    controller.enqueue(output)
  }
}

/**
 * Web Streams decoder: TOONL `string | Uint8Array` chunks in, records out.
 * It shares the same line grammar and trailer checks as `decodeLines`.
 */
export function ToonlDecodeStream() {
  const WebTransformStream = requireTransformStream()
  const state = { buffer: '', decoder: createDecodeState(), textDecoder: new TextDecoder() }

  return new WebTransformStream({
    transform(chunk, controller) {
      consumeBufferedLines(
        state,
        chunkText(state.textDecoder, chunk, true),
        controller,
        enqueueDecodedRecord,
      )
    },
    flush(controller) {
      consumeBufferedLines(state, state.textDecoder.decode(), controller, enqueueDecodedRecord)
      consumeBufferedFlush(state, controller, enqueueDecodedRecord)
    },
  })
}

/** Web Streams encoder: records in, TOONL text chunks out. */
export function ToonlEncodeStream(options) {
  const WebTransformStream = requireTransformStream()
  const state = { emitter: encodeLines(options) }

  return new WebTransformStream({
    transform(record, controller) {
      const output = state.emitter.push(record)
      if (output !== '') {
        controller.enqueue(output)
      }
    },
    flush(controller) {
      finishEmitter(state, controller)
    },
  })
}

/** JSONL text chunks in, TOONL text chunks out. */
export function JsonlToToonl(options) {
  const WebTransformStream = requireTransformStream()
  const state = { buffer: '', emitter: encodeLines(options), textDecoder: new TextDecoder() }

  return new WebTransformStream({
    transform(chunk, controller) {
      consumeBufferedLines(
        state,
        chunkText(state.textDecoder, chunk, true),
        controller,
        enqueueEncodedJson,
      )
    },
    flush(controller) {
      consumeBufferedLines(state, state.textDecoder.decode(), controller, enqueueEncodedJson)
      consumeBufferedFlush(state, controller, enqueueEncodedJson)
      finishEmitter(state, controller)
    },
  })
}

/** TOONL text chunks in, JSONL text chunks out. */
export function ToonlToJsonl() {
  const WebTransformStream = requireTransformStream()
  const state = { buffer: '', decoder: createDecodeState(), textDecoder: new TextDecoder() }

  return new WebTransformStream({
    transform(chunk, controller) {
      consumeBufferedLines(
        state,
        chunkText(state.textDecoder, chunk, true),
        controller,
        enqueueJsonlRecord,
      )
    },
    flush(controller) {
      consumeBufferedLines(state, state.textDecoder.decode(), controller, enqueueJsonlRecord)
      consumeBufferedFlush(state, controller, enqueueJsonlRecord)
    },
  })
}

/**
 * Maps or filters record streams and emits TOONL. Return `undefined` or `null`
 * to drop a record; schema rotation is handled by the output encoder.
 */
export function recordTransform(fn, options) {
  if (typeof fn !== 'function') {
    throw toonlError(0, 'recordTransform requires a function')
  }
  const WebTransformStream = requireTransformStream()
  const state = { emitter: encodeLines(options) }

  return new WebTransformStream({
    transform(record, controller) {
      const next = fn(record)
      if (next === undefined || next === null) {
        return
      }
      const output = state.emitter.push(next)
      if (output !== '') {
        controller.enqueue(output)
      }
    },
    flush(controller) {
      finishEmitter(state, controller)
    },
  })
}

/** Converts a whole JSON document string to canonical TOON. */
export function jsonToToon(input) {
  return serialize(JSON.parse(input))
}

/** Converts a whole TOON document string to compact JSON. */
export function toonToJson(input) {
  return JSON.stringify(parse(input))
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

function validateDelimiter(delimiter) {
  if (!DELIMITERS.includes(delimiter)) {
    throw toonlError(0, 'invalid header delimiter')
  }
}

function headerText(delimiter, fields) {
  const bracket = delimiter === DOCUMENT_DELIMITER ? '' : delimiter
  return `[${bracket}]{${fields.map(canonicalKey).join(delimiter)}}:\n`
}

/** Field list of a record, rejecting anything TOONL cannot put in a flat row. */
function recordFields(record) {
  if (record === null || typeof record !== 'object' || Array.isArray(record)) {
    throw toonlError(0, 'TOONL output requires object rows')
  }
  const fields = Object.keys(record)
  if (fields.length === 0) {
    throw toonlError(0, 'TOONL output requires object rows')
  }
  if (fields.some((field) => !isPrimitive(record[field]))) {
    throw toonlError(0, 'TOONL rows must be flat objects')
  }
  return fields
}

/** A single TOONL segment: fixed schema, rows appended, closed by a trailer. */
export class ToonlEncoder {
  #delimiter
  #fields
  #chunks
  #rowCount = 0

  constructor(delimiter, fields) {
    validateDelimiter(delimiter)
    if (!Array.isArray(fields) || fields.length === 0) {
      throw toonlError(0, 'TOONL header requires fields')
    }
    const names = fields.map((field) => {
      let name
      try {
        ;[name] = parseKey(String(field), 0)
      } catch (error) {
        throw asToonlError(error)
      }
      if (name === '') {
        throw toonlError(0, 'TOONL header requires fields')
      }
      return name
    })

    this.#delimiter = delimiter
    this.#fields = names
    this.#chunks = [headerText(delimiter, names)]
  }

  get fields() {
    return [...this.#fields]
  }

  get rowCount() {
    return this.#rowCount
  }

  /** Appends already-encoded cells, validating arity and each scalar. */
  pushRawRow(cells) {
    if (!Array.isArray(cells) || cells.length !== this.#fields.length) {
      throw toonlError(0, 'row arity mismatch')
    }
    for (const cell of cells) {
      try {
        parseScalar(cell, 0)
      } catch (error) {
        throw asToonlError(error)
      }
    }
    this.#chunks.push(`${cells.join(this.#delimiter)}\n`)
    this.#rowCount += 1
  }

  /** Appends a record, which must carry exactly this segment's fields. */
  pushRow(record) {
    const cells = this.#fields.map((field) => {
      if (!Object.prototype.hasOwnProperty.call(record ?? {}, field)) {
        throw toonlError(0, 'TOONL output schema changed')
      }
      if (!isPrimitive(record[field])) {
        throw toonlError(0, 'TOONL rows must be flat objects')
      }
      return primitiveText(record[field], this.#delimiter)
    })
    this.pushRawRow(cells)
  }

  /** Closes the segment with its `[=N]` trailer and returns the whole text. */
  finish() {
    return `${this.#chunks.join('')}[=${this.#rowCount}]\n`
  }

  /** The segment text so far, header included, without a trailer. */
  toString() {
    return this.#chunks.join('')
  }
}

/**
 * Incremental TOONL emitter. The header is written lazily with the first record,
 * a schema change rotates the segment automatically, and `end()` closes the last
 * one. Each call returns the text to append — nothing is buffered across calls.
 *
 * `trailer` (default `true`) writes the `[=N]` trailer when a segment closes.
 */
export function encodeLines({ delimiter = DOCUMENT_DELIMITER, trailer = true } = {}) {
  validateDelimiter(delimiter)
  let fields = null
  let rowCount = 0
  let ended = false

  const closeSegment = () => {
    if (fields === null || !trailer) {
      return ''
    }
    return `[=${rowCount}]\n`
  }

  return {
    push(record) {
      if (ended) {
        throw toonlError(0, 'TOONL encoder is closed')
      }
      const next = recordFields(record)
      let output = ''

      if (fields === null || fields.length !== next.length || fields.some((f, i) => f !== next[i])) {
        output += closeSegment()
        fields = next
        rowCount = 0
        output += headerText(delimiter, fields)
      }

      const cells = fields.map((field) => primitiveText(record[field], delimiter))
      output += `${cells.join(delimiter)}\n`
      rowCount += 1
      return output
    },

    end() {
      if (ended) {
        return ''
      }
      ended = true
      return closeSegment()
    },
  }
}

/** Convenience: encodes records to one TOONL string, rotating on schema change. */
export function encodeRecords(records, options) {
  const emitter = encodeLines(options)
  let output = ''
  for (const record of records) {
    output += emitter.push(record)
  }
  return output + emitter.end()
}
