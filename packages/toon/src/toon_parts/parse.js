import { ToonError, toonError } from '../errors.js'
import { DOCUMENT_DELIMITER, findUnquoted, parseKey, parseScalar, setKey, splitDelimited } from '../lexical.js'
import { DEFAULT_MAX_DEPTH } from './constants.js'
import { isPlainObject } from './common.js'
import { collectLines, checkDepth, checkHeaderDepth, resolveOptions } from './options.js'
import { expandCyclicDiscriminatedArrays } from './cyclic.js'
import { atLine, flattenHeaderFieldTree, lengthMismatch, parseArrayHeaderFieldTree, parseHeader, parseMapHeader, parseTabularCell } from './parse_headers.js'
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
  return resolved.cyclicDiscriminatedArrays ? expandCyclicDiscriminatedArrays(document) : document
}

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

