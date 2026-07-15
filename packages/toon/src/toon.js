/**
 * TOON (Token-Oriented Object Notation) decoder and encoder.
 *
 * Implements the v3.3 working draft hosted at https://github.com/toon-format/spec.
 * The decoder honours the spec's decoder options (`indent`, `strict`, `expandPaths`);
 * the encoder emits the canonical default profile: comma document delimiter,
 * two-space indentation, no key folding.
 */

import { ToonError, toonError } from './errors.js'
import {
  DOCUMENT_DELIMITER,
  canonicalKey,
  findUnquoted,
  isPrimitive,
  needsQuotes,
  parseKey,
  parseScalar,
  primitiveText,
  quoteString,
  setKey,
  splitDelimited,
  splitLines,
} from './lexical.js'

/** Spaces per indentation level unless `options.indent` says otherwise. */
export const DEFAULT_INDENT = 2
export const DEFAULT_MAX_DEPTH = 1000
const CYCLIC_DISCRIMINATED_ARRAY_SENTINEL = '@toon-cyclic-discriminated-array/1'

function resolveOptions(options = {}) {
  const rawMaxDepth = options.maxDepth ?? DEFAULT_MAX_DEPTH
  const maxDepth = rawMaxDepth === Number.POSITIVE_INFINITY ? 0 : Math.max(0, Math.floor(rawMaxDepth))
  return {
    indent: Math.max(1, options.indent ?? DEFAULT_INDENT),
    strict: options.strict ?? true,
    // The spec spells this `expandPaths: "safe"`; a boolean is accepted too.
    expandPaths: options.expandPaths === 'safe' || options.expandPaths === true,
    maxDepth,
  }
}

// ---------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------

function collectLines(input, options) {
  const lines = []
  let blankBefore = false

  splitLines(input).forEach((rawLine, index) => {
    const number = index + 1
    if (rawLine.trim() === '') {
      blankBefore = true
      return
    }

    const spaces = rawLine.length - rawLine.replace(/^ +/, '').length
    if (rawLine[spaces] === '\t') {
      throw toonError(number, 'invalid indentation')
    }
    if (options.strict && spaces % options.indent !== 0) {
      throw toonError(number, 'invalid indentation')
    }

    const depth = Math.floor(spaces / options.indent)
    checkDepth(depth, number, options)

    lines.push({
      number,
      depth,
      content: rawLine.slice(spaces),
      blankBefore,
    })
    blankBefore = false
  })

  return lines
}

function checkDepth(depth, line, options) {
  if (options.maxDepth !== 0 && depth > options.maxDepth) {
    throw toonError(line, `maximum nesting depth exceeded (maxDepth ${options.maxDepth})`)
  }
}

function checkHeaderDepth(header, line, options) {
  if (options.maxDepth === 0) {
    return
  }
  let depth = 0
  let inString = false
  let escaped = false
  for (const character of header) {
    if (escaped) {
      escaped = false
      continue
    }
    if (inString && character === '\\') {
      escaped = true
      continue
    }
    if (character === '"') {
      inString = !inString
      continue
    }
    if (inString) {
      continue
    }
    if (character === '{') {
      depth += 1
      checkDepth(depth, line, options)
    } else if (character === '}') {
      depth = Math.max(0, depth - 1)
    }
  }
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/** Decodes TOON per spec §5 root-form discovery. */
export function parse(input, options) {
  if (splitLines(input)[0] === CYCLIC_DISCRIMINATED_ARRAY_SENTINEL) {
    return parseCyclicDiscriminatedArrayWire(input)
  }

  const resolved = resolveOptions(options)
  const lines = collectLines(input, resolved)
  const first = lines[0]
  if (first === undefined) {
    return {}
  }
  if (first.depth !== 0) {
    throw toonError(first.number, 'invalid indentation')
  }

  const onlyLine = lines.length === 1
  if (onlyLine && first.content.trim() === '[]') {
    return []
  }

  if (first.content.startsWith('[')) {
    checkHeaderDepth(first.content, first.number, resolved)
    let header
    try {
      header = parseHeader(first.content, findUnquoted(first.content, ':', first.number))
    } catch (error) {
      if (resolved.strict) {
        throw atLine(error, first.number)
      }
      header = undefined
    }
    if (header !== undefined) {
      return parseRootArray(header, lines, resolved)
    }
  }

  if (onlyLine && findUnquoted(first.content, ':', first.number) === -1) {
    return parseScalar(first.content.trim(), first.number)
  }

  const cursor = { index: 0 }
  const document = parseObject(lines, cursor, 0, resolved)
  const trailing = lines[cursor.index]
  if (trailing !== undefined) {
    throw toonError(trailing.number, 'expected end of document')
  }
  return document
}

function parseCyclicDiscriminatedArrayWire(input) {
  const lines = splitLines(input).map((content, index) => ({ number: index + 1, content }))
  let index = 0

  index = expectCyclicLine(lines, index, CYCLIC_DISCRIMINATED_ARRAY_SENTINEL)
  const rootLine = cyclicNextLine(lines, index)
  index += 1
  const root = parseCyclicRoot(rootLine.content, rootLine.number)

  const sections = new Map()
  while (index < lines.length) {
    const line = lines[index]
    if (line.content === '@end') {
      index += 1
      break
    }
    if (!line.content.startsWith('@array ')) {
      throw cyclicInvalid(line.number)
    }
    const section = parseCyclicArraySection(lines, index)
    index = section.index
    if (sections.has(section.id)) {
      throw cyclicInvalid(line.number)
    }
    sections.set(section.id, section.value)
  }

  if (index !== lines.length) {
    throw cyclicInvalid(lines[index].number)
  }
  if (sections.size === 0) {
    throw cyclicInvalid(lines.at(-1)?.number ?? 1)
  }

  const document = {}
  for (const [key, sectionId] of root) {
    if (!sections.has(sectionId)) {
      throw cyclicInvalid(2)
    }
    setKey(document, key, sections.get(sectionId))
    sections.delete(sectionId)
  }
  return document
}

function parseCyclicRoot(content, line) {
  const json = content.startsWith('@root ') ? content.slice('@root '.length) : undefined
  if (json === undefined) {
    throw cyclicInvalid(line)
  }
  let value
  try {
    value = JSON.parse(json)
  } catch {
    throw cyclicInvalid(line)
  }
  if (!isPlainObject(value) || Object.keys(value).length === 0) {
    throw cyclicInvalid(line)
  }
  return Object.entries(value).map(([key, sectionId]) => {
    if (typeof sectionId !== 'string') {
      throw cyclicInvalid(line)
    }
    return [key, sectionId]
  })
}

function parseCyclicArraySection(lines, index) {
  const headerLine = cyclicNextLine(lines, index)
  const header = parseCyclicArrayHeader(headerLine.content, headerLine.number)
  index += 1

  const common = parseCyclicCommonRows(lines, index, header)
  index = common.index
  const groups = new Map()
  while (index < lines.length) {
    const line = lines[index]
    if (line.content === '@end' || line.content.startsWith('@array ')) {
      break
    }
    if (!line.content.startsWith('@group ')) {
      throw cyclicInvalid(line.number)
    }
    const group = parseCyclicGroup(lines, index)
    index = group.index
    if (groups.has(group.label)) {
      throw cyclicInvalid(line.number)
    }
    groups.set(group.label, group.rows)
  }

  const order = parseCyclicOrder(header.order, header.len, headerLine.number)
  const cursors = new Map()
  const values = []
  for (let position = 0; position < order.length; position += 1) {
    const label = order[position]
    const group = groups.get(label)
    if (group === undefined) {
      throw cyclicGroupLengthMismatch(headerLine.number)
    }
    const cursor = cursors.get(label) ?? 0
    const payload = group[cursor]
    if (payload === undefined) {
      throw cyclicGroupLengthMismatch(headerLine.number)
    }
    cursors.set(label, cursor + 1)
    values.push(mergeCyclicRow(header.discriminator, label, common.rows[position], payload, headerLine.number))
  }

  for (const [label, group] of groups) {
    if ((cursors.get(label) ?? 0) !== group.length) {
      throw cyclicGroupLengthMismatch(headerLine.number)
    }
  }

  return { id: header.id, value: values, index }
}

function parseCyclicArrayHeader(content, line) {
  const rest = content.startsWith('@array ') ? content.slice('@array '.length) : undefined
  if (rest === undefined) {
    throw cyclicInvalid(line)
  }
  const split = rest.indexOf(' ')
  if (split === -1) {
    throw cyclicInvalid(line)
  }
  const id = rest.slice(0, split)
  const parts = rest.slice(split + 1).trim().split(/\s+/)
  if (parts.length !== 4) {
    throw cyclicInvalid(line)
  }
  const discriminator = parts[0].startsWith('discr=') ? parts[0].slice('discr='.length) : ''
  if (discriminator === '') {
    throw cyclicInvalid(line)
  }
  if (!parts[1].startsWith('n=')) {
    throw cyclicInvalid(line)
  }
  const len = parseCyclicUsize(parts[1].slice('n='.length), line)
  const commonPart = parts[2].startsWith('common=') ? parts[2].slice('common='.length) : undefined
  if (commonPart === undefined) {
    throw cyclicInvalid(line)
  }
  const common = commonPart === '' ? [] : commonPart.split(',')
  if (common.some((field) => field === '') || new Set(common).size !== common.length) {
    throw cyclicInvalid(line)
  }
  const order = parts[3].startsWith('order=') ? parts[3].slice('order='.length) : ''
  if (order === '') {
    throw cyclicInvalid(line)
  }
  return { id, discriminator, len, common, order }
}

function parseCyclicCommonRows(lines, index, header) {
  if (header.common.length === 0) {
    if (lines[index]?.content === '@common') {
      throw cyclicInvalid(lines[index].number)
    }
    return { rows: Array.from({ length: header.len }, () => ({})), index }
  }

  index = expectCyclicLine(lines, index, '@common')
  const rows = []
  for (let rowIndex = 0; rowIndex < header.len; rowIndex += 1) {
    const line = cyclicNextLine(lines, index)
    if (line.content.startsWith('@')) {
      throw cyclicLengthMismatch(line.number)
    }
    const cells = line.content.split('\t')
    if (cells.length !== header.common.length) {
      throw cyclicLengthMismatch(line.number)
    }
    const row = {}
    header.common.forEach((key, cellIndex) => {
      setKey(row, key, parseCyclicJsonCell(cells[cellIndex], line.number))
    })
    rows.push(row)
    index += 1
  }
  return { rows, index }
}

function parseCyclicGroup(lines, index) {
  const headerLine = cyclicNextLine(lines, index)
  const { label, len } = parseCyclicGroupHeader(headerLine.content, headerLine.number)
  index += 1

  const rows = []
  for (let rowIndex = 0; rowIndex < len; rowIndex += 1) {
    const line = cyclicNextLine(lines, index)
    if (line.content.startsWith('@')) {
      throw cyclicGroupLengthMismatch(line.number)
    }
    rows.push(parseCyclicJsonObject(line.content, line.number))
    index += 1
  }
  const next = lines[index]
  if (next !== undefined && !next.content.startsWith('@')) {
    throw cyclicGroupLengthMismatch(next.number)
  }
  return { label, rows, index }
}

function parseCyclicGroupHeader(content, line) {
  const rest = content.startsWith('@group ') ? content.slice('@group '.length) : undefined
  if (rest === undefined) {
    throw cyclicInvalid(line)
  }
  const split = rest.indexOf(' n=')
  if (split === -1) {
    throw cyclicInvalid(line)
  }
  return {
    label: percentDecode(rest.slice(0, split), line),
    len: parseCyclicUsize(rest.slice(split + ' n='.length), line),
  }
}

function parseCyclicOrder(encoded, len, line) {
  if (!encoded.startsWith('cycle(')) {
    throw cyclicInvalid(line)
  }
  const rest = encoded.slice('cycle('.length)
  const split = rest.indexOf(')*')
  if (split === -1) {
    throw cyclicInvalid(line)
  }
  const cyclePart = rest.slice(0, split)
  const repeatsPart = rest.slice(split + ')*'.length)
  if (cyclePart === '' || repeatsPart.includes('+tail(')) {
    throw cyclicInvalid(line)
  }
  const cycle = cyclePart.split(',').map((label) => percentDecode(label, line))
  if (cycle.some((label) => label === '')) {
    throw cyclicInvalid(line)
  }
  const repeats = parseCyclicUsize(repeatsPart, line)
  const orderLen = cycle.length * repeats
  if (!Number.isSafeInteger(orderLen)) {
    throw cyclicInvalid(line)
  }
  if (orderLen !== len) {
    throw cyclicLengthMismatch(line)
  }
  return Array.from({ length: orderLen }, (_, index) => cycle[index % cycle.length])
}

function mergeCyclicRow(discriminator, label, common, payload, line) {
  const row = {}
  setKey(row, discriminator, label)
  if (common !== undefined) {
    for (const [key, value] of Object.entries(common)) {
      if (key === discriminator) {
        throw cyclicInvalid(line)
      }
      setKey(row, key, value)
    }
  }
  for (const [key, value] of Object.entries(payload)) {
    if (Object.prototype.hasOwnProperty.call(row, key)) {
      throw cyclicInvalid(line)
    }
    setKey(row, key, value)
  }
  return row
}

function parseCyclicJsonCell(input, line) {
  try {
    return JSON.parse(input)
  } catch {
    throw cyclicInvalid(line)
  }
}

function parseCyclicJsonObject(input, line) {
  const value = parseCyclicJsonCell(input, line)
  if (!isPlainObject(value)) {
    throw cyclicInvalid(line)
  }
  return value
}

function expectCyclicLine(lines, index, expected) {
  const line = cyclicNextLine(lines, index)
  if (line.content !== expected) {
    throw cyclicInvalid(line.number)
  }
  return index + 1
}

function cyclicNextLine(lines, index) {
  const line = lines[index]
  if (line === undefined) {
    throw cyclicInvalid((lines.at(-1)?.number ?? 0) + 1)
  }
  return line
}

function parseCyclicUsize(input, line) {
  if (input === '' || (input.length > 1 && input.startsWith('0')) || !/^[0-9]+$/.test(input)) {
    throw cyclicInvalid(line)
  }
  const value = Number(input)
  if (!Number.isSafeInteger(value)) {
    throw cyclicInvalid(line)
  }
  return value
}

function percentDecode(input, line) {
  try {
    return decodeURIComponent(input)
  } catch {
    throw cyclicInvalid(line)
  }
}

function cyclicInvalid(line) {
  return toonError(line, 'invalid cyclic array wire')
}

function cyclicLengthMismatch(line) {
  return toonError(line, 'cyclic array length mismatch')
}

function cyclicGroupLengthMismatch(line) {
  return toonError(line, 'cyclic array group length mismatch')
}

/**
 * Inspects a TOON document or TOONL stream and returns a stable truncation
 * diagnosis instead of throwing on decode.
 */
export function detectTruncation(input, options = {}) {
  const format = options.format ?? 'toon'
  if (format === 'toonl') {
    return detectToonlTruncation(input)
  }
  if (format !== 'toon') {
    throw new TypeError('detectTruncation format must be "toon" or "toonl"')
  }

  try {
    parse(input, options)
    return completeReport()
  } catch (error) {
    if (error instanceof ToonError && error.reason === 'array length mismatch') {
      return detectToonArrayTruncation(input, options, error.line)
    }
    return {
      complete: false,
      kind: 'invalid',
      line: error instanceof ToonError ? error.line : 1,
      declared: null,
      actual: null,
      message: error instanceof Error ? error.message : String(error),
    }
  }
}

/** Decodes TOON whose root form is an object. */
export function parseDocument(input, options) {
  const value = parse(input, options)
  if (!isPlainObject(value)) {
    throw toonError(1, 'expected `key: value`')
  }
  return value
}

function completeReport() {
  return { complete: true, kind: 'complete', line: null, declared: null, actual: null, message: null }
}

function truncationReport(kind, line, declared, actual, message) {
  return { complete: false, kind, line, declared, actual, message }
}

function detectToonArrayTruncation(input, options, fallbackLine) {
  const resolved = resolveOptions(options)
  const lines = collectLines(input, resolved)
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index]
    const colon = findUnquoted(line.content, ':', line.number)
    if (colon === -1 || !line.content.includes('[')) {
      continue
    }
    let header
    try {
      header = parseHeader(line.content, colon)
    } catch {
      continue
    }
    const headerDepth = line.depth
    const rowDepth = headerDepth + 1
    const valuePart = line.content.slice(colon + 1).trim()
    if (header.fields === undefined && valuePart !== '') {
      const actual = splitDelimited(valuePart, header.delimiter, line.number).length
      if (actual !== header.len) {
        return truncationReport(
          'array_length_mismatch',
          line.number,
          header.len,
          actual,
          `declared ${header.len} items but received ${actual}`,
        )
      }
      continue
    }

    let actual = 0
    for (let rowIndex = index + 1; rowIndex < lines.length; rowIndex += 1) {
      const row = lines[rowIndex]
      if (row.depth < rowDepth) {
        break
      }
      if (row.depth === rowDepth) {
        actual += 1
      }
    }
    if (actual < header.len) {
      const last = lines[lines.length - 1]
      return truncationReport(
        header.fields === undefined ? 'array_length_mismatch' : 'array_length_mismatch',
        last === undefined ? fallbackLine : last.number,
        header.len,
        actual,
        `declared ${header.len} rows but received ${actual}`,
      )
    }
  }

  const last = lines[lines.length - 1]
  return truncationReport(
    'unterminated_nesting',
    last === undefined ? fallbackLine : last.number,
    null,
    null,
    'document ended before the declared nested structure was complete',
  )
}

function detectToonlTruncation(input) {
  let open = null
  for (const [offset, rawLine] of input.split(/\n/).entries()) {
    const lineNumber = offset + 1
    const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine
    if (line === '') {
      continue
    }
    if (line.startsWith('[=') && line.endsWith(']')) {
      const digits = line.slice(2, -1)
      const declared = /^[0-9]+$/.test(digits) ? Number.parseInt(digits, 10) : null
      if (open === null) {
        return truncationReport('invalid', lineNumber, declared, null, 'trailer without header')
      }
      if (declared !== open.actual) {
        return truncationReport(
          'toonl_trailer_count_mismatch',
          lineNumber,
          declared,
          open.actual,
          `trailer declared ${declared} rows but received ${open.actual}`,
        )
      }
      open = null
      continue
    }
    if (line.startsWith('[') && line.endsWith(':') && open === null) {
      open = { line: lineNumber, actual: 0 }
      continue
    }
    if (open !== null) {
      open.actual += 1
    }
  }
  if (open !== null) {
    return truncationReport(
      'toonl_missing_trailer',
      open.line,
      null,
      open.actual,
      `stream ended without a trailer after ${open.actual} rows`,
    )
  }
  return completeReport()
}

function parseObject(lines, cursor, depth, options) {
  const document = {}

  while (cursor.index < lines.length) {
    const line = lines[cursor.index]
    if (line.depth < depth) {
      break
    }
    if (line.depth > depth) {
      throw toonError(line.number, 'invalid indentation')
    }

    const { key, quoted, value } = parseField(lines, cursor, depth, options)
    insertField(document, key, quoted, value, options, line.number)
  }

  return document
}

/** Parses one `key: value` line (and any body it owns), advancing the cursor. */
function parseField(lines, cursor, depth, options) {
  const line = lines[cursor.index]
  const content = line.content
  const colon = findUnquoted(content, ':', line.number)
  if (colon === -1) {
    throw toonError(line.number, 'expected `key: value`')
  }
  const keyPart = content.slice(0, colon)
  const valuePart = content.slice(colon + 1)

  const arrayOpen = findUnquoted(keyPart, '[', line.number)
  const mapOpen = findUnquoted(keyPart, '{', line.number)
  if (mapOpen !== -1 && (arrayOpen === -1 || mapOpen < arrayOpen)) {
    checkHeaderDepth(keyPart, line.number, options)
    let header
    try {
      header = parseMapHeader(keyPart)
    } catch (error) {
      if (options.strict) {
        throw atLine(error, line.number)
      }
      cursor.index += 1
      const value = parseFieldValue(lines, cursor, depth, valuePart, line.number, options)
      return { key: keyPart.trim(), quoted: false, value }
    }

    if (valuePart.trim() !== '') {
      throw toonError(line.number, 'expected keyed map rows')
    }
    const value = parseKeyedMapRows(header, lines, cursor, depth + 1, options)
    return { key: header.key, quoted: header.keyQuoted, value }
  }

  if (arrayOpen !== -1) {
    checkHeaderDepth(keyPart, line.number, options)
    let header
    try {
      header = parseHeader(keyPart, colon)
    } catch (error) {
      if (options.strict) {
        throw atLine(error, line.number)
      }
      // Non-strict decoders fall through to key-value parsing with the whole
      // prefix as a literal key (spec §6).
      cursor.index += 1
      const value = parseFieldValue(lines, cursor, depth, valuePart, line.number, options)
      return { key: keyPart.trim(), quoted: false, value }
    }

    if (header.key === '' && !header.keyQuoted) {
      throw toonError(line.number, 'expected non-empty field name')
    }
    const value = parseArrayField(header, valuePart, lines, cursor, depth, options)
    return { key: header.key, quoted: header.keyQuoted, value }
  }

  const [key, quoted] = parseKey(keyPart, line.number)
  if (key === '' && !quoted) {
    throw toonError(line.number, 'expected non-empty field name')
  }
  cursor.index += 1
  const value = parseFieldValue(lines, cursor, depth, valuePart, line.number, options)
  return { key, quoted, value }
}

/** Value of a non-header field. The cursor already points past the field's own line. */
function parseFieldValue(lines, cursor, depth, valuePart, line, options) {
  const text = valuePart.trim()
  if (text === '[]') {
    return []
  }
  if (text !== '') {
    return parseScalar(text, line)
  }

  // A bare `key:` opens a nested — possibly empty — object, never an array (§8).
  const next = lines[cursor.index]
  if (next !== undefined && next.depth > depth) {
    return parseObject(lines, cursor, depth + 1, options)
  }
  return {}
}

function parseKeyedMapRows(header, lines, cursor, rowDepth, options) {
  const document = {}
  cursor.index += 1

  while (cursor.index < lines.length) {
    const line = lines[cursor.index]
    if (line.depth < rowDepth) {
      break
    }
    if (line.depth > rowDepth) {
      throw toonError(line.number, 'invalid indentation')
    }
    if (line.blankBefore && options.strict) {
      throw toonError(line.number, 'blank line inside keyed map')
    }

    const colon = findUnquoted(line.content, ':', line.number)
    if (colon === -1) {
      throw toonError(line.number, 'expected `key: value`')
    }
    const [key, quoted] = parseKey(line.content.slice(0, colon), line.number)
    if (key === '' && !quoted) {
      throw toonError(line.number, 'expected non-empty field name')
    }
    const cells = splitDelimited(line.content.slice(colon + 1).trim(), header.delimiter, line.number)
    if (cells.length !== header.fields.length) {
      throw toonError(line.number, 'keyed map row length mismatch')
    }

    const row = {}
    header.fields.forEach((field, index) => {
      insertPath(row, field.path, parseTabularCell(field, cells[index], line.number), options, line.number)
    })
    insertField(document, key, quoted, row, options, line.number)
    cursor.index += 1
  }

  return document
}

// ---------------------------------------------------------------------------
// Arrays
// ---------------------------------------------------------------------------

function parseRootArray(header, lines, options) {
  const first = lines[0]
  const colon = findUnquoted(first.content, ':', first.number)
  if (colon === -1) {
    throw toonError(first.number, 'expected `key: value`')
  }
  const valuePart = first.content.slice(colon + 1)
  const cursor = { index: 0 }
  const value = parseArrayField(header, valuePart, lines, cursor, 0, options)
  const trailing = lines[cursor.index]
  if (trailing !== undefined) {
    throw toonError(trailing.number, 'expected end of document')
  }
  return value
}

/** Reads an array declared by `header`; the cursor points at the header line. */
function parseArrayField(header, valuePart, lines, cursor, headerDepth, options) {
  const headerLine = lines[cursor.index].number
  const inline = valuePart.trim()
  cursor.index += 1

  if (header.fields !== undefined) {
    if (inline !== '') {
      throw toonError(headerLine, 'expected tabular rows')
    }
    return parseTabularRows(header, header.fields, lines, cursor, headerDepth + 1, options)
  }

  if (inline !== '') {
    const values = splitDelimited(inline, header.delimiter, headerLine).map((value) =>
      parseScalar(value, headerLine),
    )
    if (values.length !== header.len) {
      throw toonError(headerLine, 'array length mismatch')
    }
    return values
  }

  return parseListItems(header, lines, cursor, headerDepth + 1, options)
}

function parseTabularRows(header, fields, lines, cursor, rowDepth, options) {
  if (header.fieldTree !== undefined && hasComplexHeaderFields(header.fieldTree)) {
    return parseStructuredTabularRows(header, lines, cursor, rowDepth, options)
  }

  const rows = []

  while (rows.length < header.len) {
    const line = lines[cursor.index]
    if (line === undefined) {
      throw lengthMismatch(lines, cursor.index)
    }
    if (line.depth < rowDepth) {
      break
    }
    if (line.depth > rowDepth) {
      throw toonError(line.number, 'invalid indentation')
    }
    if (line.blankBefore && options.strict) {
      throw toonError(line.number, 'blank line inside array')
    }
    if (!isTabularRow(line.content, header.delimiter, line.number)) {
      break
    }

    const cells = splitDelimited(line.content, header.delimiter, line.number)
    if (cells.length !== fields.length) {
      throw toonError(line.number, 'array row length mismatch')
    }
    const row = {}
    fields.forEach((field, index) => {
      insertPath(row, field.path, parseTabularCell(field, cells[index], line.number), options, line.number)
    })
    rows.push(row)
    cursor.index += 1
  }

  if (rows.length !== header.len) {
    throw lengthMismatch(lines, cursor.index)
  }
  const next = lines[cursor.index]
  if (
    next !== undefined &&
    next.depth >= rowDepth &&
    isTabularRow(next.content, header.delimiter, next.number)
  ) {
    throw toonError(next.number, 'array length mismatch')
  }

  return rows
}

function parseStructuredTabularRows(header, lines, cursor, rowDepth, options) {
  return parseStructuredRows(header.len, header.fieldTree, header.delimiter, lines, cursor, rowDepth, options, true)
}

function parseStructuredRows(len, fields, delimiter, lines, cursor, rowDepth, options, root) {
  const rows = []
  const childTableFields = inferChildTableFields(
    len,
    fields,
    delimiter,
    lines,
    cursor.index,
    rowDepth,
    options,
  )

  while (rows.length < len) {
    const line = lines[cursor.index]
    if (line === undefined) {
      throw lengthMismatch(lines, cursor.index)
    }
    if (line.depth < rowDepth) {
      break
    }
    if (line.depth > rowDepth) {
      throw toonError(line.number, 'invalid indentation')
    }
    if (line.blankBefore && options.strict) {
      throw toonError(line.number, 'blank line inside array')
    }
    if (!isTabularRow(line.content, delimiter, line.number)) {
      break
    }

    const cells = splitDelimited(line.content, delimiter, line.number)
    const state = {
      cellIndex: 0,
      nextIndex: cursor.index + 1,
      flatWidth: leafWidth(fields),
      childTableFields,
    }
    const row = parseStructuredRowFields(fields, cells, line.number, lines, state, rowDepth + 1, delimiter, options)
    if (state.cellIndex !== cells.length) {
      throw toonError(line.number, 'array row length mismatch')
    }
    rows.push(matrixRowValue(fields, row, root))
    cursor.index = state.nextIndex
  }

  if (rows.length !== len) {
    throw lengthMismatch(lines, cursor.index)
  }
  const next = lines[cursor.index]
  if (
    next !== undefined &&
    next.depth >= rowDepth &&
    isTabularRow(next.content, delimiter, next.number)
  ) {
    throw toonError(next.number, 'array length mismatch')
  }

  return rows
}

function parseStructuredRowFields(fields, cells, line, lines, state, childDepth, delimiter, options) {
  const row = {}
  fields.forEach((field, index) => {
    const remainingFields = fields.slice(index + 1)
    const value = parseStructuredField(field, remainingFields, cells, line, lines, state, childDepth, delimiter, options)
    insertPath(row, [field.key], value, options, line)
  })
  return row
}

function parseStructuredField(field, remainingFields, cells, line, lines, state, childDepth, delimiter, options) {
  if (field.fixedLength !== undefined) {
    if (state.cellIndex + field.fixedLength > cells.length) {
      throw toonError(line, 'array row length mismatch')
    }
    const values = cells
      .slice(state.cellIndex, state.cellIndex + field.fixedLength)
      .map((cell) => parseScalar(cell, line))
    state.cellIndex += field.fixedLength
    return values
  }

  if (field.children !== undefined) {
    const flatWidth = leafWidth(field.children)
    const countCell = cells[state.cellIndex]
    const count = parseChildCount(countCell)
    const cellsAfterChildCount = cells.length - state.cellIndex - 1
    const childLine = lines[state.nextIndex]
    const hasChildRows = childLine !== undefined && childLine.depth === childDepth
    const knownChildTable =
      state.childTableFields === undefined ? undefined : state.childTableFields.has(field)
    const mustBeChildTable = knownChildTable === undefined
      ? count !== undefined &&
        (hasChildRows ||
          (cells.length !== state.flatWidth &&
            cellsAfterChildCount < flatWidth + minimumRowWidth(remainingFields)))
      : knownChildTable

    if (mustBeChildTable) {
      if (count === undefined) {
        throw toonError(line, 'array row length mismatch')
      }
      state.cellIndex += 1
      return parseStructuredRows(
        count,
        field.children,
        delimiter,
        lines,
        { get index() { return state.nextIndex }, set index(value) { state.nextIndex = value } },
        childDepth,
        options,
        false,
      )
    }

    return parseStructuredRowFields(field.children, cells, line, lines, state, childDepth, delimiter, options)
  }

  const cell = cells[state.cellIndex]
  if (cell === undefined) {
    throw toonError(line, 'array row length mismatch')
  }
  state.cellIndex += 1
  return parseTabularCell(field, cell, line)
}

function inferChildTableFields(len, fields, delimiter, lines, startIndex, rowDepth, options) {
  const candidates = fields.filter((field) => field.children !== undefined)
  if (candidates.length === 0) {
    return new Set()
  }
  if (candidates.length > 12) {
    return undefined
  }

  let best
  const combinations = 1 << candidates.length
  for (let mask = 0; mask < combinations; mask += 1) {
    const childTableFields = new Set()
    candidates.forEach((field, index) => {
      if ((mask & (1 << index)) !== 0) {
        childTableFields.add(field)
      }
    })
    const result = validateStructuredRowsWithKind(
      len,
      fields,
      childTableFields,
      delimiter,
      lines,
      startIndex,
      rowDepth,
      options,
    )
    if (result === undefined) {
      continue
    }
    if (
      best === undefined ||
      result.consumedChildRows > best.consumedChildRows ||
      (result.consumedChildRows === best.consumedChildRows && childTableFields.size < best.childTableFields.size)
    ) {
      best = { childTableFields, consumedChildRows: result.consumedChildRows }
    }
  }

  return best?.childTableFields
}

function validateStructuredRowsWithKind(
  len,
  fields,
  childTableFields,
  delimiter,
  lines,
  startIndex,
  rowDepth,
  options,
) {
  let index = startIndex
  let consumedChildRows = 0

  try {
    for (let row = 0; row < len; row += 1) {
      const line = lines[index]
      if (
        line === undefined ||
        line.depth !== rowDepth ||
        (line.blankBefore && options.strict) ||
        !isTabularRow(line.content, delimiter, line.number)
      ) {
        return undefined
      }

      const cells = splitDelimited(line.content, delimiter, line.number)
      const result = validateStructuredRowWithKind(
        fields,
        childTableFields,
        cells,
        delimiter,
        lines,
        index + 1,
        rowDepth + 1,
        options,
      )
      if (result === undefined) {
        return undefined
      }
      if (lines[result.nextIndex]?.depth > rowDepth) {
        return undefined
      }
      consumedChildRows += result.consumedChildRows
      index = result.nextIndex
    }

    const next = lines[index]
    if (next !== undefined && next.depth >= rowDepth && isTabularRow(next.content, delimiter, next.number)) {
      return undefined
    }
  } catch {
    return undefined
  }

  return { nextIndex: index, consumedChildRows }
}

function validateStructuredRowWithKind(
  fields,
  childTableFields,
  cells,
  delimiter,
  lines,
  startIndex,
  childDepth,
  options,
) {
  let cellIndex = 0
  let nextIndex = startIndex
  let consumedChildRows = 0

  for (const field of fields) {
    if (field.fixedLength !== undefined) {
      cellIndex += field.fixedLength
    } else if (field.children !== undefined) {
      if (childTableFields.has(field)) {
        const count = parseChildCount(cells[cellIndex])
        if (count === undefined) {
          return undefined
        }
        cellIndex += 1
        const nestedChildTableFields = inferChildTableFields(
          count,
          field.children,
          delimiter,
          lines,
          nextIndex,
          childDepth,
          options,
        )
        if (nestedChildTableFields === undefined) {
          return undefined
        }
        const result = validateStructuredRowsWithKind(
          count,
          field.children,
          nestedChildTableFields,
          delimiter,
          lines,
          nextIndex,
          childDepth,
          options,
        )
        if (result === undefined) {
          return undefined
        }
        nextIndex = result.nextIndex
        consumedChildRows += count + result.consumedChildRows
      } else {
        cellIndex += leafWidth(field.children)
      }
    } else {
      cellIndex += 1
    }

    if (cellIndex > cells.length) {
      return undefined
    }
  }

  if (cellIndex !== cells.length) {
    return undefined
  }

  return { nextIndex, consumedChildRows }
}

function matrixRowValue(fields, row, root) {
  if (root && fields.length === 1 && fields[0].fixedLength !== undefined) {
    return row[fields[0].key]
  }
  return row
}

function parseChildCount(value) {
  if (value === undefined || !/^(0|[1-9][0-9]*)$/.test(value)) {
    return undefined
  }
  return Number.parseInt(value, 10)
}

function hasComplexHeaderFields(fields) {
  return fields.some((field) => field.fixedLength !== undefined || field.children !== undefined)
}

function leafWidth(fields) {
  return fields.reduce((total, field) => total + fieldWidth(field), 0)
}

function fieldWidth(field) {
  if (field.fixedLength !== undefined) {
    return field.fixedLength
  }
  if (field.children !== undefined) {
    return leafWidth(field.children)
  }
  return 1
}

function minimumRowWidth(fields) {
  return fields.reduce((total, field) => {
    if (field.children !== undefined) {
      return total + 1
    }
    return total + fieldWidth(field)
  }, 0)
}

/**
 * Spec §9.3 row disambiguation: a same-depth line is a row unless an unquoted
 * colon precedes the first unquoted active delimiter.
 */
function isTabularRow(content, delimiter, line) {
  const colon = findUnquoted(content, ':', line)
  if (colon === -1) {
    return true
  }
  const delimiterIndex = findUnquoted(content, delimiter, line)
  return delimiterIndex === -1 ? false : delimiterIndex < colon
}

function parseListItems(header, lines, cursor, itemDepth, options) {
  const values = []

  while (values.length < header.len) {
    const line = lines[cursor.index]
    if (line === undefined || line.depth < itemDepth) {
      throw lengthMismatch(lines, cursor.index)
    }
    if (line.depth > itemDepth) {
      throw toonError(line.number, 'invalid indentation')
    }
    if (line.blankBefore && options.strict) {
      throw toonError(line.number, 'blank line inside array')
    }
    values.push(parseListItem(lines, cursor, itemDepth, options))
  }

  const next = lines[cursor.index]
  if (next !== undefined && next.depth >= itemDepth) {
    throw toonError(next.number, 'array length mismatch')
  }

  return values
}

function parseListItem(lines, cursor, itemDepth, options) {
  const line = lines[cursor.index]
  if (!line.content.startsWith('-')) {
    throw toonError(line.number, 'expected array item')
  }
  const inner = line.content.slice(1).replace(/^\s+/, '')

  // Bare `-`: an empty object list item (§10).
  if (inner === '') {
    cursor.index += 1
    return {}
  }

  // `- [M]: …`: a nested array whose body sits one level under the hyphen (§9.4).
  if (inner.startsWith('[')) {
    const colon = findUnquoted(inner, ':', line.number)
    checkHeaderDepth(inner, line.number, options)
    let header
    try {
      header = parseHeader(inner, colon)
    } catch {
      header = undefined
    }
    if (header !== undefined) {
      const valuePart = inner.slice(colon + 1)
      return parseArrayField(header, valuePart, lines, cursor, itemDepth, options)
    }
  }

  // `- key: …`: an object whose fields live at the hyphen's content column.
  if (findUnquoted(inner, ':', line.number) !== -1) {
    const itemLines = [
      { number: line.number, depth: itemDepth + 1, content: inner, blankBefore: false },
    ]
    cursor.index += 1
    while (cursor.index < lines.length) {
      const next = lines[cursor.index]
      if (next.depth <= itemDepth) {
        break
      }
      itemLines.push(next)
      cursor.index += 1
    }

    return parseObject(itemLines, { index: 0 }, itemDepth + 1, options)
  }

  cursor.index += 1
  return parseScalar(inner, line.number)
}

function lengthMismatch(lines, index) {
  const line = lines[index] ?? lines[lines.length - 1]
  return toonError(line === undefined ? 1 : line.number, 'array length mismatch')
}

// ---------------------------------------------------------------------------
// Headers
// ---------------------------------------------------------------------------

/**
 * Parses `key[N<delim?>]{fields}:` (spec §6). `colon` is the first unquoted colon
 * on the line; the header must terminate exactly there. Throws with `line: 0`, so
 * callers stamp their own line number via {@link atLine}.
 */
function parseHeader(content, colon) {
  if (colon === -1) {
    throw toonError(0, 'array header missing colon')
  }
  const keyPart = content.slice(0, colon)

  const open = findUnquoted(keyPart, '[', 0)
  if (open === -1) {
    throw toonError(0, 'invalid array header')
  }
  let key
  let keyQuoted
  try {
    ;[key, keyQuoted] = parseKey(keyPart.slice(0, open), 0)
  } catch {
    throw toonError(0, 'invalid array header')
  }

  const rest = keyPart.slice(open + 1)
  const close = rest.indexOf(']')
  if (close === -1) {
    throw toonError(0, 'invalid array header')
  }
  const bracket = rest.slice(0, close)

  const digits = /^[0-9]*/.exec(bracket)[0]
  if (digits === '' || (digits.length > 1 && digits.startsWith('0'))) {
    throw toonError(0, 'invalid array header')
  }
  const len = Number.parseInt(digits, 10)

  let delimiter
  switch (bracket.slice(digits.length)) {
    case '':
      delimiter = ','
      break
    case '\t':
      delimiter = '\t'
      break
    case '|':
      delimiter = '|'
      break
    default:
      throw toonError(0, 'invalid array header')
  }

  const suffix = rest.slice(close + 1)
  let fields
  let fieldTree
  if (suffix === '') {
    fields = undefined
    fieldTree = undefined
  } else if (suffix.startsWith('{') && suffix.endsWith('}') && suffix.length >= 2) {
    try {
      fieldTree = parseArrayHeaderFieldTree(suffix.slice(1, -1), delimiter)
      fields = flattenHeaderFieldTree(fieldTree)
    } catch (error) {
      throw toonError(0, error.reason === 'duplicate key' ? 'duplicate key' : 'invalid array header')
    }
  } else {
    throw toonError(0, 'invalid array header')
  }

  return { key, keyQuoted, len, delimiter, fields, fieldTree }
}

function parseArrayHeaderFields(source, delimiter) {
  if (delimiter !== DOCUMENT_DELIMITER && source.includes(DOCUMENT_DELIMITER) && source.includes('[')) {
    return parseHeaderFields(source, DOCUMENT_DELIMITER, delimiter)
  }
  try {
    return parseHeaderFields(source, delimiter, delimiter)
  } catch (error) {
    if (delimiter !== DOCUMENT_DELIMITER && error.reason !== 'duplicate key') {
      return parseHeaderFields(source, DOCUMENT_DELIMITER, delimiter)
    }
    throw error
  }
}

function parseMapHeader(content) {
  const open = findUnquoted(content, '{', 0)
  if (open === -1 || !content.endsWith('}')) {
    throw toonError(0, 'invalid keyed map header')
  }
  let key
  let keyQuoted
  try {
    ;[key, keyQuoted] = parseKey(content.slice(0, open), 0)
  } catch {
    throw toonError(0, 'invalid keyed map header')
  }
  if (key === '' && !keyQuoted) {
    throw toonError(0, 'expected non-empty field name')
  }

  let fieldText = content.slice(open + 1, -1)
  let delimiter = DOCUMENT_DELIMITER
  if (fieldText.startsWith('|') || fieldText.startsWith('\t')) {
    delimiter = fieldText[0]
    fieldText = fieldText.slice(1)
  }

  let fields
  try {
    fields = parseHeaderFields(fieldText, delimiter, delimiter)
  } catch (error) {
    throw toonError(0, error.reason === 'duplicate key' ? 'duplicate key' : 'invalid keyed map header')
  }
  return { key, keyQuoted, delimiter, fields }
}

function parseHeaderFields(source, delimiter, activeDelimiter) {
  const fields = []
  let index = 0

  function parseList(prefix, nested) {
    let count = 0

    while (index < source.length) {
      if (nested && source[index] === '}') {
        break
      }
      if (source[index] === delimiter || source[index] === '}') {
        throw toonError(0, 'invalid array header')
      }

      const start = index
      while (index < source.length) {
        const character = source[index]
        if (character === '"') {
          skipQuotedHeaderKey()
          continue
        }
        if (character === delimiter || character === '{' || character === '}' || character === '[') {
          break
        }
        index += 1
      }

      const [key] = parseKey(source.slice(start, index), 0)
      if (key === '') {
        throw toonError(0, 'invalid array header')
      }

      count += 1
      if (source[index] === '[') {
        index += 1
        const delimiterStart = index
        while (index < source.length && source[index] !== ']') {
          index += 1
        }
        if (index >= source.length) {
          throw toonError(0, 'invalid array header')
        }
        const listDelimiter = source.slice(delimiterStart, index)
        if (!isValidListDelimiter(listDelimiter, activeDelimiter)) {
          throw toonError(0, 'invalid array header')
        }
        index += 1
        fields.push({ path: [...prefix, key], listDelimiter })
      } else if (source[index] === '{') {
        index += 1
        const before = fields.length
        parseList([...prefix, key], true)
        if (source[index] !== '}' || fields.length === before) {
          throw toonError(0, 'invalid array header')
        }
        index += 1
      } else {
        fields.push({ path: [...prefix, key], listDelimiter: undefined })
      }

      if (source[index] === delimiter) {
        index += 1
        if (index >= source.length || source[index] === '}') {
          throw toonError(0, 'invalid array header')
        }
        continue
      }
      if (nested ? source[index] === '}' : index === source.length) {
        break
      }
      throw toonError(0, 'invalid array header')
    }

    if (count === 0) {
      throw toonError(0, 'invalid array header')
    }
  }

  function skipQuotedHeaderKey() {
    index += 1
    while (index < source.length) {
      const character = source[index]
      index += 1
      if (character === '\\') {
        index += 1
      } else if (character === '"') {
        return
      }
    }
  }

  parseList([], false)
  if (index !== source.length) {
    throw toonError(0, 'invalid array header')
  }

  const seen = new Set()
  for (let index = 0; index < fields.length; index += 1) {
    for (let other = index + 1; other < fields.length; other += 1) {
      if (
        samePath(fields[index].path, fields[other].path) ||
        pathStartsWith(fields[index].path, fields[other].path) ||
        pathStartsWith(fields[other].path, fields[index].path)
      ) {
        throw toonError(0, 'duplicate key')
      }
    }
  }
  for (const field of fields) {
    const path = field.path
    const key = path.join('\u0000')
    if (seen.has(key)) {
      throw toonError(0, 'duplicate key')
    }
    seen.add(key)
  }
  return fields
}

function parseArrayHeaderFieldTree(source, delimiter) {
  if (
    delimiter !== DOCUMENT_DELIMITER &&
    source.includes(DOCUMENT_DELIMITER) &&
    (source.includes('[') || source.includes('{'))
  ) {
    return parseHeaderFieldTree(source, DOCUMENT_DELIMITER, delimiter)
  }
  try {
    return parseHeaderFieldTree(source, delimiter, delimiter)
  } catch (error) {
    if (delimiter !== DOCUMENT_DELIMITER && error.reason !== 'duplicate key') {
      return parseHeaderFieldTree(source, DOCUMENT_DELIMITER, delimiter)
    }
    throw error
  }
}

function parseHeaderFieldTree(source, delimiter, activeDelimiter) {
  let index = 0

  function parseList(nested) {
    const fields = []

    while (index < source.length) {
      if (nested && source[index] === '}') {
        break
      }
      if (source[index] === delimiter || source[index] === '}') {
        throw toonError(0, 'invalid array header')
      }

      const start = index
      while (index < source.length) {
        const character = source[index]
        if (character === '"') {
          skipQuotedHeaderKey()
          continue
        }
        if (character === delimiter || character === '{' || character === '}' || character === '[') {
          break
        }
        index += 1
      }

      const [key] = parseKey(source.slice(start, index), 0)
      if (key === '') {
        throw toonError(0, 'invalid array header')
      }

      const field = { key }
      if (source[index] === '[') {
        index += 1
        const bracketStart = index
        while (index < source.length && source[index] !== ']') {
          index += 1
        }
        if (index >= source.length) {
          throw toonError(0, 'invalid array header')
        }
        const bracket = source.slice(bracketStart, index)
        index += 1

        const fixed = parseFixedWidthList(bracket)
        if (fixed !== undefined) {
          if (fixed.delimiter !== activeDelimiter) {
            throw toonError(0, 'invalid array header')
          }
          field.fixedLength = fixed.len
          field.fixedDelimiter = fixed.delimiter
        } else if (isValidListDelimiter(bracket, activeDelimiter)) {
          field.listDelimiter = bracket
        } else {
          throw toonError(0, 'invalid array header')
        }
      } else if (source[index] === '{') {
        index += 1
        field.children = parseList(true)
        if (source[index] !== '}' || field.children.length === 0) {
          throw toonError(0, 'invalid array header')
        }
        index += 1
      }

      fields.push(field)

      if (source[index] === delimiter) {
        index += 1
        if (index >= source.length || source[index] === '}') {
          throw toonError(0, 'invalid array header')
        }
        continue
      }
      if (nested ? source[index] === '}' : index === source.length) {
        break
      }
      throw toonError(0, 'invalid array header')
    }

    if (fields.length === 0) {
      throw toonError(0, 'invalid array header')
    }
    return fields
  }

  function skipQuotedHeaderKey() {
    index += 1
    while (index < source.length) {
      const character = source[index]
      index += 1
      if (character === '\\') {
        index += 1
      } else if (character === '"') {
        return
      }
    }
  }

  const fields = parseList(false)
  if (index !== source.length) {
    throw toonError(0, 'invalid array header')
  }

  const paths = flattenHeaderFieldTree(fields)
  for (let index = 0; index < paths.length; index += 1) {
    for (let other = index + 1; other < paths.length; other += 1) {
      if (
        samePath(paths[index].path, paths[other].path) ||
        pathStartsWith(paths[index].path, paths[other].path) ||
        pathStartsWith(paths[other].path, paths[index].path)
      ) {
        throw toonError(0, 'duplicate key')
      }
    }
  }
  return fields
}

function parseFixedWidthList(value) {
  const digits = /^[0-9]+/.exec(value)?.[0]
  if (digits === undefined || (digits.length > 1 && digits.startsWith('0'))) {
    return undefined
  }
  const suffix = value.slice(digits.length)
  let delimiter
  switch (suffix) {
    case '':
      delimiter = DOCUMENT_DELIMITER
      break
    case '\t':
      delimiter = '\t'
      break
    case '|':
      delimiter = '|'
      break
    default:
      return undefined
  }
  return { len: Number.parseInt(digits, 10), delimiter }
}

function flattenHeaderFieldTree(fields, prefix = []) {
  return fields.flatMap((field) => {
    const path = [...prefix, field.key]
    if (field.children !== undefined) {
      return flattenHeaderFieldTree(field.children, path)
    }
    return [{ path, listDelimiter: field.listDelimiter }]
  })
}

function parseTabularCell(field, cell, line) {
  if (field.listDelimiter === undefined) {
    return parseScalar(cell, line)
  }
  return splitDelimited(cell, field.listDelimiter, line).map((value) => parseScalar(value, line))
}

function isValidListDelimiter(value, activeDelimiter) {
  return (
    value.length === 1 &&
    value !== activeDelimiter &&
    !/[ \t\r\n"[\]{}:]/.test(value)
  )
}

function samePath(left, right) {
  return left.length === right.length && pathStartsWith(left, right)
}

function pathStartsWith(path, prefix) {
  return prefix.every((segment, index) => path[index] === segment)
}

function atLine(error, line) {
  return toonError(line, error.reason ?? String(error.message))
}

// ---------------------------------------------------------------------------
// Field insertion, duplicate keys and path expansion
// ---------------------------------------------------------------------------

function insertField(document, key, quoted, value, options, line) {
  if (options.expandPaths && !quoted && key.includes('.')) {
    const segments = key.split('.')
    if (segments.every(isIdentifierSegment)) {
      insertPath(document, segments, value, options, line)
      return
    }
  }

  insertPath(document, [key], value, options, line)
}

function insertPath(document, segments, value, options, line) {
  checkDepth(segments.length - 1, line, options)

  const key = segments[0]
  const exists = Object.prototype.hasOwnProperty.call(document, key)

  if (segments.length === 1) {
    if (exists && options.strict) {
      throw toonError(line, 'duplicate key')
    }
    // Last write wins in non-strict mode (§14.3, §14.4).
    setKey(document, key, value)
    return
  }

  if (exists && !isPlainObject(document[key])) {
    if (options.strict) {
      throw toonError(line, 'path expansion conflict')
    }
    setKey(document, key, {})
  } else if (!exists) {
    setKey(document, key, {})
  }

  insertPath(document[key], segments.slice(1), value, options, line)
}

function isIdentifierSegment(segment) {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(segment)
}

export function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/** Encodes a JSON value as canonical TOON (default profile). */
export function serialize(value, options = {}) {
  const rawMaxDepth = options.maxDepth ?? DEFAULT_MAX_DEPTH
  const delimiter = options.delimiter ?? DOCUMENT_DELIMITER
  if (![DOCUMENT_DELIMITER, '|', '\t'].includes(delimiter)) {
    throw toonError(0, 'invalid array header')
  }
  const resolved = {
    nestedTabularHeaders: options.nestedTabularHeaders === true,
    keyedMapCollapse: options.keyedMapCollapse === true,
    primitiveArrayColumns: options.primitiveArrayColumns === true,
    objectArrayColumns: options.objectArrayColumns === true,
    delimiter,
    maxDepth: rawMaxDepth === Number.POSITIVE_INFINITY ? 0 : Math.max(0, Math.floor(rawMaxDepth)),
  }
  const output = []
  if (Array.isArray(value)) {
    writeArray(output, undefined, value, 0, false, resolved)
  } else if (isPlainObject(value)) {
    writeFields(output, value, 0, resolved)
  } else {
    output.push(primitiveText(value, delimiter))
  }
  return output.join('')
}

function writeIndent(output, depth) {
  output.push(' '.repeat(depth * DEFAULT_INDENT))
}

function writeFields(output, document, depth, options) {
  checkDepth(depth, 0, options)
  for (const [key, value] of Object.entries(document)) {
    writeIndent(output, depth)
    writeField(output, key, value, depth, options)
  }
}

/** Writes `key: value` at the caller's cursor (indent or `- ` already emitted). */
function writeField(output, key, value, depth, options) {
  checkDepth(depth, 0, options)
  if (Array.isArray(value)) {
    writeArray(output, key, value, depth, false, options)
    return
  }
  if (isPlainObject(value)) {
    const shape = options.keyedMapCollapse ? keyedMapShape(value, options, depth + 1) : undefined
    if (shape !== undefined) {
      output.push(
        canonicalKey(key),
        '{',
        delimiterPrefix(options.delimiter),
        shape.fields.map((field) => headerFieldText(field, options.delimiter)).join(options.delimiter),
        '}:\n',
      )
      for (const [rowKey, rowValue] of Object.entries(value)) {
        writeIndent(output, depth + 1)
        const childOutput = []
        output.push(
          canonicalKey(rowKey),
          ': ',
          shape.paths
            .map((column) => columnText(valueAtPath(rowValue, column.path), column, options.delimiter, options, childOutput, depth + 2))
            .join(options.delimiter),
          '\n',
        )
        output.push(...childOutput)
      }
      return
    }
    output.push(canonicalKey(key), ':\n')
    writeFields(output, value, depth + 1, options)
    return
  }
  output.push(canonicalKey(key), ': ', primitiveText(value, options.delimiter), '\n')
}

/**
 * Writes an array header plus body. `listItem` selects the empty-array form:
 * `[0]:` inside a list (§9.2) versus `key: []` / `[]` elsewhere (§9.1).
 */
function writeArray(output, key, values, depth, listItem, options) {
  checkDepth(depth, 0, options)
  if (values.length === 0) {
    if (key !== undefined) {
      output.push(canonicalKey(key), ': []\n')
    } else if (listItem) {
      output.push('[0]:\n')
    } else {
      output.push('[]\n')
    }
    return
  }

  if (values.every(isPrimitive)) {
    writeArrayHeader(output, key, values.length, undefined, options.delimiter)
    output.push(' ')
    output.push(values.map((value) => primitiveText(value, options.delimiter)).join(options.delimiter))
    output.push('\n')
    return
  }

  // In list-item position the tabular form has nowhere to put its field list, so
  // §9.4 requires the expanded list even for a uniform array of objects.
  const shape = listItem ? undefined : tabularShape(values, options, depth + 1)
  if (shape !== undefined) {
    writeArrayHeader(output, key, values.length, shape.fields, options.delimiter)
    output.push('\n')
    for (const value of values) {
      writeIndent(output, depth + 1)
      const childOutput = []
      output.push(
        shape.paths
          .map((column) => columnText(valueAtPath(value, column.path), column, options.delimiter, options, childOutput, depth + 2))
          .join(options.delimiter),
      )
      output.push('\n')
      output.push(...childOutput)
    }
    return
  }

  writeArrayHeader(output, key, values.length, undefined, options.delimiter)
  output.push('\n')
  for (const value of values) {
    writeIndent(output, depth + 1)
    writeListItem(output, value, depth + 1, options)
  }
}

function writeListItem(output, value, depth, options) {
  checkDepth(depth, 0, options)
  if (Array.isArray(value)) {
    output.push('- ')
    writeArray(output, undefined, value, depth, true, options)
    return
  }
  if (isPlainObject(value)) {
    const entries = Object.entries(value)
    if (entries.length === 0) {
      output.push('-\n')
      return
    }
    output.push('- ')
    writeField(output, entries[0][0], entries[0][1], depth + 1, options)
    for (const [key, nested] of entries.slice(1)) {
      writeIndent(output, depth + 1)
      writeField(output, key, nested, depth + 1, options)
    }
    return
  }
  output.push('- ', primitiveText(value, options.delimiter), '\n')
}

function writeArrayHeader(output, key, len, fields, delimiter) {
  if (key !== undefined) {
    output.push(canonicalKey(key))
  }
  output.push('[', String(len), delimiterPrefix(delimiter), ']')
  if (fields !== undefined) {
    output.push('{', fields.map((field) => headerFieldText(field, delimiter)).join(delimiter), '}')
  }
  output.push(':')
}

function delimiterPrefix(delimiter) {
  return delimiter === DOCUMENT_DELIMITER ? '' : delimiter
}

/**
 * Tabular eligibility (§9.3): every element is a non-empty object, all share the
 * first element's key set, and every value is primitive.
 */
export function tabularFields(values) {
  const shape = tabularShape(values, { nestedTabularHeaders: false, maxDepth: DEFAULT_MAX_DEPTH }, 1)
  return shape?.fields.map((field) => field.key)
}

function tabularShape(values, options, depth) {
  const matrix = matrixShape(values, options)
  if (matrix !== undefined) {
    return matrix
  }
  const shape = objectShape(values.map((value) => ({ value })), options, depth)
  return shape === undefined ? undefined : { fields: shape, paths: leafPaths(shape) }
}

function keyedMapShape(document, options, depth) {
  const values = Object.values(document)
  if (values.length < 2) {
    return undefined
  }
  return tabularShape(values, options, depth)
}

function objectShape(records, options, depth) {
  checkDepth(depth, 0, options)
  const first = records[0]?.value
  if (!isPlainObject(first)) {
    return undefined
  }
  const fields = Object.keys(first).map((key) => ({ key }))

  if (fields.length === 0) {
    return undefined
  }

  for (const { value } of records) {
    if (!isPlainObject(value)) {
      return undefined
    }
    const keys = Object.keys(value)
    if (keys.length !== fields.length) {
      return undefined
    }
    if (fields.some((field) => !Object.prototype.hasOwnProperty.call(value, field.key))) {
      return undefined
    }
  }

  for (const field of fields) {
    const cells = records.map(({ value }) => ({ value: value[field.key] }))
    if (cells.every(({ value }) => isPrimitive(value))) {
      continue
    }
    if (
      options.primitiveArrayColumns &&
      cells.every(({ value }) => Array.isArray(value) && value.every(isPrimitive))
    ) {
      field.listDelimiter = ';'
      continue
    }
    if (options.objectArrayColumns && cells.every(({ value }) => Array.isArray(value))) {
      const matrix = matrixColumnShape(cells.map(({ value }) => value))
      if (matrix !== undefined) {
        field.fixedLength = matrix.fixedLength
        field.fixedDelimiter = options.delimiter
        continue
      }
      const children = objectShape(
        cells.flatMap(({ value }) => value.map((child) => ({ value: child }))),
        options,
        depth + 1,
      )
      if (children !== undefined) {
        field.children = children
        field.childTable = true
        continue
      }
    }
    if (!options.nestedTabularHeaders) {
      return undefined
    }
    const children = objectShape(cells, options, depth + 1)
    if (children === undefined) {
      return undefined
    }
    field.children = children
  }

  return fields
}

function matrixShape(values, options) {
  if (!options.objectArrayColumns) {
    return undefined
  }
  const matrix = matrixColumnShape(values)
  if (matrix === undefined) {
    return undefined
  }
  const fields = [{ key: 'values', fixedLength: matrix.fixedLength, fixedDelimiter: options.delimiter }]
  return { fields, paths: [{ path: [], fixedLength: matrix.fixedLength }] }
}

function matrixColumnShape(values) {
  const firstLength = values[0]?.length
  if (!Number.isInteger(firstLength) || firstLength === 0) {
    return undefined
  }
  if (!values.every((value) => Array.isArray(value) && value.length === firstLength && value.every(isPrimitive))) {
    return undefined
  }
  return { fixedLength: firstLength }
}

function leafPaths(fields, prefix = []) {
  return fields.flatMap((field) => {
    const path = [...prefix, field.key]
    if (field.childTable === true) {
      return [{ path, childFields: field.children }]
    }
    if (field.fixedLength !== undefined) {
      return [{ path, fixedLength: field.fixedLength }]
    }
    return field.children === undefined
      ? [{ path, listDelimiter: field.listDelimiter }]
      : leafPaths(field.children, path)
  })
}

function valueAtPath(value, path) {
  let cursor = value
  for (const segment of path) {
    cursor = cursor[segment]
  }
  return cursor
}

function headerFieldText(field, delimiter) {
  if (field.listDelimiter !== undefined) {
    return `${canonicalKey(field.key)}[${field.listDelimiter}]`
  }
  if (field.fixedLength !== undefined) {
    return `${canonicalKey(field.key)}[${field.fixedLength}${delimiterPrefix(delimiter)}]`
  }
  if (field.children === undefined) {
    return canonicalKey(field.key)
  }
  return `${canonicalKey(field.key)}{${field.children.map((child) => headerFieldText(child, delimiter)).join(delimiter)}}`
}

function columnText(value, column, activeDelimiter, options, output, childDepth) {
  if (column.childFields !== undefined) {
    writeChildRows(output, value, column.childFields, options, childDepth)
    return String(value.length)
  }
  if (column.fixedLength !== undefined) {
    return value.map((item) => primitiveText(item, activeDelimiter)).join(activeDelimiter)
  }
  if (column.listDelimiter === undefined) {
    return primitiveText(value, activeDelimiter)
  }
  return value.map((item) => primitiveListItemText(item, activeDelimiter, column.listDelimiter)).join(column.listDelimiter)
}

function writeChildRows(output, values, fields, options, depth) {
  for (const value of values) {
    writeIndent(output, depth)
    const nestedOutput = []
    output.push(
      leafPaths(fields)
        .map((column) => columnText(valueAtPath(value, column.path), column, options.delimiter, options, nestedOutput, depth + 1))
        .join(options.delimiter),
    )
    output.push('\n')
    output.push(...nestedOutput)
  }
}

function primitiveListItemText(value, activeDelimiter, listDelimiter) {
  if (typeof value !== 'string') {
    return primitiveText(value, activeDelimiter)
  }
  return needsQuotes(value, activeDelimiter) || value.includes(listDelimiter) ? quoteString(value) : value
}
