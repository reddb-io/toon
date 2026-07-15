/**
 * TOONL v0.1 — the append-only, line-oriented streaming profile of TOON.
 * Semantics follow `docs/toonl-v0.1.md`: a stream is a sequence of segments,
 * each opened by a `[<delim?>]{fields}:` header, filled with one row per line,
 * and optionally closed by a `[=N]` trailer that asserts the row count.
 */

import { ToonlCursorInvalidationError, asToonlError, toonlError } from './errors.js'
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
const CURSOR_ANCHOR_LIMIT = 64
const TAGGED_LANE_LIMIT = 8
const TAG_PATTERN = /^[A-Za-z0-9_-]+$/
const TEXT_ENCODER = new TextEncoder()
const TEXT_DECODER = new TextDecoder()

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

function validateTag(tag, lineNumber) {
  if (!TAG_PATTERN.test(tag)) {
    throw toonlError(lineNumber, 'invalid tag')
  }
}

function taggedRowPrefix(line, lineNumber) {
  const colon = line.indexOf(':')
  if (colon <= 0) {
    return null
  }
  const tag = line.slice(0, colon)
  if (!TAG_PATTERN.test(tag)) {
    if (/^[A-Za-z0-9_-]/.test(tag) && /^[^,[\]|{}:\t ]+$/.test(tag)) {
      throw toonlError(lineNumber, 'invalid tag')
    }
    return null
  }
  return { tag, cellsText: line.slice(colon + 1) }
}

/** `[<delim?>]{fields}:` or `[]<tag>{fields}:` → header; `null` when not a header. */
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
  const continuation = bracket.startsWith('~')
  const delimiterText = continuation ? bracket.slice(1) : bracket
  if (!continuation && delimiterText.startsWith('=')) {
    return null
  }
  if (!DELIMITERS.includes(delimiterText) && delimiterText !== '') {
    throw toonlError(lineNumber, 'invalid header delimiter')
  }
  const delimiter = delimiterText === '' ? DOCUMENT_DELIMITER : delimiterText

  let suffix = rest.slice(close + 1)
  let tag = null
  if (!continuation && suffix.startsWith('<')) {
    const tagEnd = suffix.indexOf('>')
    if (tagEnd === -1) {
      throw toonlError(lineNumber, 'invalid tag')
    }
    tag = suffix.slice(1, tagEnd)
    validateTag(tag, lineNumber)
    if (delimiterText !== '') {
      throw toonlError(lineNumber, 'invalid header delimiter')
    }
    suffix = suffix.slice(tagEnd + 1)
  }
  if (!suffix.startsWith('{') || !suffix.endsWith('}:')) {
    throw toonlError(lineNumber, 'invalid header')
  }

  const fieldText = suffix.slice(1, -2)
  let fields
  try {
    fields = splitDelimited(fieldText, delimiter, lineNumber).map((field) => parseKey(field, lineNumber)[0])
  } catch (error) {
    throw asToonlError(error)
  }
  if (fields.length === 0 || fields.some((field) => field === '')) {
    throw toonlError(lineNumber, 'invalid header fields')
  }

  return { delimiter, fields, fieldText, continuation, tag }
}

function assertContinuation(open, header, lineNumber) {
  if (open === null || open.delimiter === undefined) {
    throw toonlError(lineNumber, 'continuation header before header')
  }
  if (open.delimiter !== header.delimiter || open.fieldText !== header.fieldText) {
    throw toonlError(lineNumber, 'continuation header mismatch')
  }
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
    if (open === null || open.delimiter === undefined) {
      throw toonlError(lineNumber, 'trailer without header')
    }
    if (open.rowCount !== count) {
      throw toonlError(lineNumber, 'trailer count mismatch')
    }
    return { kind: 'trailer' }
  }

  const header = parseHeaderLine(line, lineNumber)
  if (header !== null) {
    if (header.continuation) {
      assertContinuation(open, header, lineNumber)
      return { kind: 'continuation' }
    }
    if (header.tag !== null) {
      return { kind: 'tag-header', header }
    }
    return { kind: 'header', header }
  }

  const tagged = taggedRowPrefix(line, lineNumber)
  if (tagged !== null) {
    const lane = open?.taggedLanes?.get(tagged.tag)
    if (lane === undefined) {
      throw toonlError(lineNumber, 'unknown tag')
    }
    return {
      kind: 'tagged-row',
      tag: tagged.tag,
      cells: parseRow(tagged.cellsText, lane.delimiter, lane.fields.length, lineNumber),
    }
  }

  if (open === null || open.delimiter === undefined) {
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
function parseStreamInternal(input, options = {}) {
  const laneOrder = []
  const lanes = new Map()
  const anonymousSegments = []
  const interleavedSegments = []
  let anonymous = null
  let anonymousDeclared = false
  let sawTaggedSyntax = false

  function appendInterleavedRow(segment, row) {
    const last = interleavedSegments.at(-1)
    if (
      last !== undefined &&
      last.lane === segment.lane &&
      last.delimiter === segment.delimiter &&
      last.fieldText === segment.fieldText
    ) {
      last.rows.push(row)
      return
    }
    interleavedSegments.push({
      lane: segment.lane,
      delimiter: segment.delimiter,
      fields: segment.fields,
      fieldText: segment.fieldText,
      rows: [row],
    })
  }

  function pushAnonymousSegment() {
    if (anonymous === null) {
      return
    }
    anonymousSegments.push({
      delimiter: anonymous.delimiter,
      fields: anonymous.fields,
      fieldText: anonymous.fieldText,
      lane: anonymous.lane,
      rows: anonymous.rows,
    })
  }

  function ensureLane(tag, header, lineNumber) {
    let lane = lanes.get(tag)
    if (lane === undefined) {
      if (lanes.size >= TAGGED_LANE_LIMIT) {
        throw toonlError(lineNumber, 'too many tagged lanes')
      }
      lane = { segments: [], current: null }
      lanes.set(tag, lane)
      laneOrder.push(tag)
    } else if (lane.current !== null) {
      lane.segments.push(lane.current)
    }
    lane.current = {
      lane: tag,
      delimiter: header.delimiter,
      fields: header.fields,
      fieldText: header.fieldText,
      rows: [],
    }
    lane.delimiter = header.delimiter
    lane.fields = header.fields
    lane.fieldText = header.fieldText
  }

  splitLines(input).forEach((line, index) => {
    const lineNumber = index + 1
    if (line === '') {
      return
    }

    const open =
      anonymous === null
        ? { taggedLanes: lanes }
        : {
            delimiter: anonymous.delimiter,
            fields: anonymous.fields,
            fieldText: anonymous.fieldText,
            rowCount: anonymous.rows.length,
            taggedLanes: lanes,
          }
    const step = classifyLine(line, lineNumber, open)

    if (step.kind === 'trailer') {
      pushAnonymousSegment()
      anonymous = null
      return
    }
    if (step.kind === 'continuation') {
      return
    }
    if (step.kind === 'tag-header') {
      sawTaggedSyntax = true
      ensureLane(step.header.tag, step.header, lineNumber)
      return
    }
    if (step.kind === 'header') {
      pushAnonymousSegment()
      if (!anonymousDeclared) {
        laneOrder.push(null)
        anonymousDeclared = true
      }
      anonymous = {
        lane: null,
        delimiter: step.header.delimiter,
        fields: step.header.fields,
        fieldText: step.header.fieldText,
        rows: [],
      }
      return
    }
    if (step.kind === 'tagged-row') {
      sawTaggedSyntax = true
      const segment = lanes.get(step.tag).current
      segment.rows.push(step.cells)
      appendInterleavedRow(segment, step.cells)
      return
    }
    if (anonymous === null) {
      throw toonlError(lineNumber, 'row before header')
    }
    anonymous.rows.push(step.cells)
    appendInterleavedRow(anonymous, step.cells)
  })

  if (!sawTaggedSyntax) {
    pushAnonymousSegment()
    return options.preserveInterleaving === true ? anonymousSegments : anonymousSegments
  }

  const output = []
  pushAnonymousSegment()
  for (const laneKey of laneOrder) {
    if (laneKey === null) {
      output.push(...anonymousSegments)
      continue
    }
    const lane = lanes.get(laneKey)
    output.push(...lane.segments)
    if (lane.current !== null) {
      output.push(lane.current)
    }
  }
  return options.preserveInterleaving === true ? interleavedSegments : output
}

export function parseStream(input) {
  return parseStreamInternal(input).map(({ delimiter, fields, rows }) => ({ delimiter, fields, rows }))
}

/** Every row of every segment, decoded into records. */
export function parseRecords(input) {
  return parseStreamInternal(input).flatMap((segment) =>
    segment.rows.map((row) => rowRecord(segment.fields, row, 0)),
  )
}

/**
 * Closes the stream: each segment becomes one canonical TOON document, so a
 * length-free append-only stream turns into length-bearing TOON (§ close
 * transform). Cells are already canonical, so they are re-emitted verbatim.
 */
export function closeTransform(input) {
  return parseStreamInternal(input).map((segment) => {
    const delimiter = segment.delimiter
    const bracket = delimiter === DOCUMENT_DELIMITER ? '' : delimiter
    const fields = segment.fields.map(canonicalKey).join(delimiter)
    const rows = segment.rows.map((row) => `  ${row.join(delimiter)}\n`).join('')
    return `[${segment.rows.length}${bracket}]{${fields}}:\n${rows}`
  })
}

export function closeTransformInterleaved(input) {
  return parseStreamInternal(input, { preserveInterleaving: true }).map((segment) => {
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
  yield* new ToonlReader(source)
}

function createDecodeState(cursor = null) {
  return {
    open: cursor
      ? parseCursorHeader(cursor)
      : null,
    lineNumber: 0,
    byteOffset: cursor ? cursor.byteOffset : 0,
    activeHeaderLine: cursor ? cursor.activeHeaderLine : null,
    rowsSinceHeader: cursor ? cursor.rowsSinceHeader : 0,
    anchor: cursor?.anchor ?? null,
    taggedLanes: new Map(),
  }
}

function normalizeHeaderLine(header) {
  return headerText(header.delimiter, header.fieldText, false)
}

function parseCursorHeader(cursor) {
  if (!Number.isSafeInteger(cursor?.byteOffset) || cursor.byteOffset < 0) {
    throw toonlError(0, 'invalid cursor byteOffset')
  }
  if (!Number.isSafeInteger(cursor?.rowsSinceHeader) || cursor.rowsSinceHeader < 0) {
    throw toonlError(0, 'invalid cursor rowsSinceHeader')
  }
  if (typeof cursor.activeHeaderLine !== 'string' || cursor.activeHeaderLine === '') {
    throw toonlError(0, 'invalid cursor activeHeaderLine')
  }
  const header = parseHeaderLine(cursor.activeHeaderLine.trimEnd(), 0)
  if (header === null || header.continuation || header.tag !== null) {
    throw toonlError(0, 'invalid cursor activeHeaderLine')
  }
  return {
    delimiter: header.delimiter,
    fields: header.fields,
    fieldText: header.fieldText,
    rowCount: cursor.rowsSinceHeader,
  }
}

function bytesOf(text) {
  return TEXT_ENCODER.encode(text)
}

function byteLength(text) {
  return bytesOf(text).length
}

function cursorAnchor(lineStartOffset, rawLine) {
  if (rawLine.length <= CURSOR_ANCHOR_LIMIT) {
    return { byteOffset: lineStartOffset, bytes: rawLine }
  }
  const bytes = bytesOf(rawLine)
  const start = Math.max(0, bytes.length - CURSOR_ANCHOR_LIMIT)
  return {
    byteOffset: lineStartOffset + start,
    bytes: TEXT_DECODER.decode(bytes.slice(start)),
  }
}

function currentCursor(state) {
  if (state.activeHeaderLine === null) {
    return null
  }
  return {
    byteOffset: state.byteOffset,
    activeHeaderLine: state.activeHeaderLine,
    rowsSinceHeader: state.rowsSinceHeader,
    ...(state.anchor === null ? {} : { anchor: state.anchor }),
  }
}

function validateResumeBytes(bytes, cursor) {
  if (bytes.length < cursor.byteOffset) {
    throw new ToonlCursorInvalidationError('truncated', 'TOONL cursor invalidated by truncation', {
      byteOffset: cursor.byteOffset,
      fileSize: bytes.length,
    })
  }
  if (cursor.anchor !== undefined) {
    const { byteOffset, bytes: expected } = cursor.anchor
    const expectedBytes = bytesOf(String(expected))
    const actual = bytes.slice(byteOffset, byteOffset + expectedBytes.length)
    if (actual.length !== expectedBytes.length || TEXT_DECODER.decode(actual) !== expected) {
      throw new ToonlCursorInvalidationError(
        'anchor-mismatch',
        'TOONL cursor invalidated by anchor mismatch',
        { byteOffset },
      )
    }
  }
}

function sourceBytesForResume(source) {
  if (typeof source === 'string') {
    return bytesOf(source)
  }
  if (source instanceof Uint8Array) {
    return source
  }
  return null
}

function consumeDecodeLine(state, line, rawLine = `${line}\n`, lineStartOffset = state.byteOffset) {
  state.lineNumber += 1
  state.byteOffset = lineStartOffset + byteLength(rawLine)
  state.anchor = cursorAnchor(lineStartOffset, rawLine)
  if (line === '') {
    return undefined
  }

  const open =
    state.open === null
      ? { taggedLanes: state.taggedLanes }
      : { ...state.open, taggedLanes: state.taggedLanes }
  const step = classifyLine(line, state.lineNumber, open)
  if (step.kind === 'trailer') {
    state.open = null
    return undefined
  }
  if (step.kind === 'continuation') {
    return undefined
  }
  if (step.kind === 'header') {
    state.activeHeaderLine = normalizeHeaderLine(step.header)
    state.rowsSinceHeader = 0
    state.open = {
      delimiter: step.header.delimiter,
      fields: step.header.fields,
      fieldText: step.header.fieldText,
      rowCount: 0,
    }
    return undefined
  }
  if (step.kind === 'tag-header') {
    const tag = step.header.tag
    if (!state.taggedLanes.has(tag) && state.taggedLanes.size >= TAGGED_LANE_LIMIT) {
      throw toonlError(state.lineNumber, 'too many tagged lanes')
    }
    state.taggedLanes.set(tag, {
      delimiter: step.header.delimiter,
      fields: step.header.fields,
      fieldText: step.header.fieldText,
      rowCount: 0,
    })
    return undefined
  }
  if (step.kind === 'tagged-row') {
    const lane = state.taggedLanes.get(step.tag)
    lane.rowCount += 1
    return rowRecord(lane.fields, step.cells, state.lineNumber)
  }

  state.open.rowCount += 1
  state.rowsSinceHeader += 1
  return rowRecord(state.open.fields, step.cells, state.lineNumber)
}

async function* toLineEntries(source, initialByteOffset = 0) {
  let byteOffset = initialByteOffset
  if (typeof source === 'string') {
    const hadTrailingNewline = source.endsWith('\n')
    const rawLines = source.split('\n')
    if (hadTrailingNewline) {
      rawLines.pop()
    }
    for (let index = 0; index < rawLines.length; index += 1) {
      const raw = rawLines[index]
      const rawLine = `${raw}${index < rawLines.length - 1 || hadTrailingNewline ? '\n' : ''}`
      const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw
      const lineStartOffset = byteOffset
      byteOffset += byteLength(rawLine)
      yield { line, rawLine, lineStartOffset }
    }
    return
  }

  for await (const line of toLines(source)) {
    const rawLine = `${line}\n`
    const lineStartOffset = byteOffset
    byteOffset += byteLength(rawLine)
    yield { line, rawLine, lineStartOffset }
  }
}

export class ToonlReader {
  #source
  #cursor
  #state

  constructor(source, options = {}) {
    this.#source = source
    this.#cursor = options.cursor ?? null
    this.#state = createDecodeState(this.#cursor)
  }

  get cursor() {
    const cursor = currentCursor(this.#state)
    return cursor === null ? null : JSON.parse(JSON.stringify(cursor))
  }

  async *[Symbol.asyncIterator]() {
    let source = this.#source
    let initialByteOffset = 0

    if (this.#cursor !== null) {
      const bytes = sourceBytesForResume(source)
      if (bytes === null) {
        throw toonlError(0, 'cursor resume requires a string or Uint8Array source')
      }
      validateResumeBytes(bytes, this.#cursor)
      source = TEXT_DECODER.decode(bytes.slice(this.#cursor.byteOffset))
      initialByteOffset = this.#cursor.byteOffset
    }

    for await (const { line, rawLine, lineStartOffset } of toLineEntries(source, initialByteOffset)) {
      const record = consumeDecodeLine(this.#state, line, rawLine, lineStartOffset)
      if (record !== undefined) {
        yield record
      }
    }
  }
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

function headerText(delimiter, fieldText, continuation = false) {
  const bracket = `${continuation ? '~' : ''}${delimiter === DOCUMENT_DELIMITER ? '' : delimiter}`
  return `[${bracket}]{${fieldText}}:\n`
}

function taggedHeaderText(tag, fieldText) {
  return `[]<${tag}>{${fieldText}}:\n`
}

function headerFieldText(delimiter, fields) {
  return fields.map(canonicalKey).join(delimiter)
}

function normalizeHeaderFields(fields) {
  if (!Array.isArray(fields) || fields.length === 0) {
    throw toonlError(0, 'TOONL header requires fields')
  }
  return fields.map((field) => {
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
}

function sameFields(left, right) {
  return left !== null && left.length === right.length && left.every((field, index) => field === right[index])
}

function validateCadence(cadence) {
  if (cadence !== undefined && (!Number.isInteger(cadence) || cadence <= 0)) {
    throw toonlError(0, 'TOONL continuation cadence must be positive')
  }
}

function continuationDue(options, rowsSinceContinuation, bytesSinceContinuation) {
  return (
    (options.continuationEveryRows !== undefined &&
      rowsSinceContinuation >= options.continuationEveryRows) ||
    (options.continuationEveryBytes !== undefined &&
      bytesSinceContinuation >= options.continuationEveryBytes)
  )
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

function shapeKey(fields) {
  return JSON.stringify([...fields].sort())
}

function canonicalFieldsForShape(fields, fieldsByShape) {
  const key = shapeKey(fields)
  const canonical = fieldsByShape.get(key)
  if (canonical !== undefined) {
    return canonical
  }
  fieldsByShape.set(key, fields)
  return fields
}

/** A single TOONL segment: fixed schema, rows appended, closed by a trailer. */
export class ToonlEncoder {
  #delimiter
  #fields
  #fieldText
  #chunks
  #rowCount = 0
  #rowsSinceContinuation = 0
  #bytesSinceContinuation = 0
  #options

  constructor(delimiter, fields, options = {}) {
    validateDelimiter(delimiter)
    validateCadence(options.continuationEveryRows)
    validateCadence(options.continuationEveryBytes)
    const names = normalizeHeaderFields(fields)

    this.#delimiter = delimiter
    this.#fields = names
    this.#fieldText = headerFieldText(delimiter, names)
    this.#chunks = [headerText(delimiter, this.#fieldText)]
    this.#options = { ...options }
  }

  get fields() {
    return [...this.#fields]
  }

  get rowCount() {
    return this.#rowCount
  }

  setContinuationEveryRows(rows) {
    validateCadence(rows)
    this.#options.continuationEveryRows = rows
  }

  setContinuationEveryBytes(bytes) {
    validateCadence(bytes)
    this.#options.continuationEveryBytes = bytes
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
    this.#pushContinuationIfDue()
    const row = `${cells.join(this.#delimiter)}\n`
    this.#chunks.push(row)
    this.#rowCount += 1
    this.#rowsSinceContinuation += 1
    this.#bytesSinceContinuation += row.length
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

  #pushContinuationIfDue() {
    if (!continuationDue(this.#options, this.#rowsSinceContinuation, this.#bytesSinceContinuation)) {
      return
    }
    this.#chunks.push(headerText(this.#delimiter, this.#fieldText, true))
    this.#rowsSinceContinuation = 0
    this.#bytesSinceContinuation = 0
  }
}

/**
 * Incremental TOONL emitter. The header is written lazily with the first record,
 * a schema change rotates the segment automatically, and `end()` closes the last
 * one. Each call returns the text to append — nothing is buffered across calls.
 * Field order is canonicalized per record shape using the first order seen for
 * that shape, so later records with the same field set do not rotate solely
 * because their object keys arrived in a different order.
 *
 * `trailer` (default `true`) writes the `[=N]` trailer when a segment closes.
 */
export function encodeLines({
  delimiter = DOCUMENT_DELIMITER,
  trailer = true,
  continuationEveryRows,
  continuationEveryBytes,
} = {}) {
  validateDelimiter(delimiter)
  validateCadence(continuationEveryRows)
  validateCadence(continuationEveryBytes)
  let fields = null
  let fieldText = ''
  const fieldsByShape = new Map()
  let rowCount = 0
  let rowsSinceContinuation = 0
  let bytesSinceContinuation = 0
  const taggedLanes = new Map()
  let ended = false
  const continuationOptions = { continuationEveryRows, continuationEveryBytes }

  const closeSegment = () => {
    if (fields === null || !trailer) {
      return ''
    }
    return `[=${rowCount}]\n`
  }

  const continuationHeader = () => {
    if (!continuationDue(continuationOptions, rowsSinceContinuation, bytesSinceContinuation)) {
      return ''
    }
    rowsSinceContinuation = 0
    bytesSinceContinuation = 0
    return headerText(delimiter, fieldText, true)
  }

  const ensureTaggedLane = (tag) => {
    validateTag(tag, 0)
    let lane = taggedLanes.get(tag)
    if (lane === undefined) {
      if (taggedLanes.size >= TAGGED_LANE_LIMIT) {
        throw toonlError(0, 'too many tagged lanes')
      }
      lane = { fields: null, fieldsByShape: new Map() }
      taggedLanes.set(tag, lane)
    }
    return lane
  }

  return {
    push(record) {
      if (ended) {
        throw toonlError(0, 'TOONL encoder is closed')
      }
      const next = canonicalFieldsForShape(recordFields(record), fieldsByShape)
      let output = ''

      if (fields === null || fields.length !== next.length || fields.some((f, i) => f !== next[i])) {
        output += closeSegment()
        fields = next
        fieldText = headerFieldText(delimiter, fields)
        rowCount = 0
        rowsSinceContinuation = 0
        bytesSinceContinuation = 0
        output += headerText(delimiter, fieldText)
      }

      const cells = fields.map((field) => primitiveText(record[field], delimiter))
      output += continuationHeader()
      const row = `${cells.join(delimiter)}\n`
      output += row
      rowCount += 1
      rowsSinceContinuation += 1
      bytesSinceContinuation += row.length
      return output
    },

    declareLane(tag, declaredFields) {
      if (ended) {
        throw toonlError(0, 'TOONL encoder is closed')
      }
      const names = normalizeHeaderFields(declaredFields)
      const fieldText = headerFieldText(DOCUMENT_DELIMITER, names)
      const lane = ensureTaggedLane(tag)
      if (sameFields(lane.fields, names)) {
        return ''
      }
      lane.fields = names
      return taggedHeaderText(tag, fieldText)
    },

    pushTagged(tag, record) {
      if (ended) {
        throw toonlError(0, 'TOONL encoder is closed')
      }
      const lane = ensureTaggedLane(tag)
      const next = canonicalFieldsForShape(recordFields(record), lane.fieldsByShape)
      const fieldText = headerFieldText(DOCUMENT_DELIMITER, next)
      const cells = next.map((field) => primitiveText(record[field], DOCUMENT_DELIMITER))
      let output = ''

      if (!sameFields(lane.fields, next)) {
        lane.fields = next
        output += taggedHeaderText(tag, fieldText)
      }

      return `${output}${tag}:${cells.join(DOCUMENT_DELIMITER)}\n`
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

/**
 * Convenience: encodes records to one TOONL string, rotating on schema change.
 * Uses the same first-seen per-shape field order as `encodeLines`.
 */
export function encodeRecords(records, options) {
  const emitter = encodeLines(options)
  let output = ''
  for (const record of records) {
    output += emitter.push(record)
  }
  return output + emitter.end()
}
