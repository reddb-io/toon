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
  parseKey,
  parseScalar,
  primitiveText,
  setKey,
  splitDelimited,
  splitLines,
} from './lexical.js'

/** Spaces per indentation level unless `options.indent` says otherwise. */
export const DEFAULT_INDENT = 2
export const DEFAULT_MAX_DEPTH = 1000

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
      insertPath(row, field, parseScalar(cells[index], line.number), options, line.number)
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
      insertPath(row, field, parseScalar(cells[index], line.number), options, line.number)
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
  if (suffix === '') {
    fields = undefined
  } else if (suffix.startsWith('{') && suffix.endsWith('}') && suffix.length >= 2) {
    try {
      fields = parseHeaderFields(suffix.slice(1, -1), delimiter)
    } catch (error) {
      throw toonError(0, error.reason === 'duplicate key' ? 'duplicate key' : 'invalid array header')
    }
  } else {
    throw toonError(0, 'invalid array header')
  }

  return { key, keyQuoted, len, delimiter, fields }
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
    fields = parseHeaderFields(fieldText, delimiter)
  } catch (error) {
    throw toonError(0, error.reason === 'duplicate key' ? 'duplicate key' : 'invalid keyed map header')
  }
  return { key, keyQuoted, delimiter, fields }
}

function parseHeaderFields(source, delimiter) {
  const paths = []
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
        if (character === delimiter || character === '{' || character === '}') {
          break
        }
        index += 1
      }

      const [key] = parseKey(source.slice(start, index), 0)
      if (key === '') {
        throw toonError(0, 'invalid array header')
      }

      count += 1
      if (source[index] === '{') {
        index += 1
        const before = paths.length
        parseList([...prefix, key], true)
        if (source[index] !== '}' || paths.length === before) {
          throw toonError(0, 'invalid array header')
        }
        index += 1
      } else {
        paths.push([...prefix, key])
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
  for (let index = 0; index < paths.length; index += 1) {
    for (let other = index + 1; other < paths.length; other += 1) {
      if (
        samePath(paths[index], paths[other]) ||
        pathStartsWith(paths[index], paths[other]) ||
        pathStartsWith(paths[other], paths[index])
      ) {
        throw toonError(0, 'duplicate key')
      }
    }
  }
  for (const path of paths) {
    const key = path.join('\u0000')
    if (seen.has(key)) {
      throw toonError(0, 'duplicate key')
    }
    seen.add(key)
  }
  return paths
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
        output.push(
          canonicalKey(rowKey),
          ': ',
          shape.paths
            .map((path) => primitiveText(valueAtPath(rowValue, path), options.delimiter))
            .join(options.delimiter),
          '\n',
        )
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
      output.push(
        shape.paths
          .map((path) => primitiveText(valueAtPath(value, path), options.delimiter))
          .join(options.delimiter),
      )
      output.push('\n')
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

function leafPaths(fields, prefix = []) {
  return fields.flatMap((field) => {
    const path = [...prefix, field.key]
    return field.children === undefined ? [path] : leafPaths(field.children, path)
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
  if (field.children === undefined) {
    return canonicalKey(field.key)
  }
  return `${canonicalKey(field.key)}{${field.children.map((child) => headerFieldText(child, delimiter)).join(delimiter)}}`
}
