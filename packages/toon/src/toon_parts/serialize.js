import { toonError } from '../errors.js'
import { DOCUMENT_DELIMITER, canonicalKey, isPrimitive, needsQuotes, primitiveText, quoteString } from '../lexical.js'
import { DEFAULT_INDENT, DEFAULT_MAX_DEPTH } from './constants.js'
import { isPlainObject } from './common.js'
import { checkDepth } from './options.js'
import { cyclicDiscriminatedArrayWire } from './cyclic.js'
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
    cyclicDiscriminatedArrays: options.cyclicDiscriminatedArrays === true,
    delimiter,
    maxDepth: rawMaxDepth === Number.POSITIVE_INFINITY ? 0 : Math.max(0, Math.floor(rawMaxDepth)),
  }
  if (resolved.cyclicDiscriminatedArrays) {
    const cyclic = cyclicDiscriminatedArrayWire(value)
    if (cyclic !== undefined) {
      return cyclic
    }
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
