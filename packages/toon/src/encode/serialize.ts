import { canonicalKey, needsQuotes, primitiveText, quoteString } from '../lexical.js'
import { toonError } from '../errors.js'
import { DEFAULT_MAX_DEPTH } from '../constants.js'
import { isPlainObject, isPrimitive, normalizeValue } from './normalize.js'
import { applyReplacer, type EncodeReplacer } from './replacer.js'
import { keyedFields, tabularFields, type FieldNode } from './shape.js'
import { cyclicDiscriminatedArrayWire } from '../cyclic.js'

export interface EncodeOptions {
  delimiter?: ',' | '|' | '\t'
  indentSize?: number
  /** @deprecated Use indentSize. */
  indent?: number
  replacer?: EncodeReplacer
  cyclicDiscriminatedArrays?: boolean
  primitiveArrayColumns?: boolean
  objectArrayColumns?: boolean
  maxDepth?: number
}

interface ResolvedOptions {
  delimiter: ',' | '|' | '\t'
  indentSize: number
  maxDepth: number
  primitiveArrayColumns: boolean
  objectArrayColumns: boolean
}

/** Encodes normalized JSON using the canonical v4.1 forms. */
export function encode(input: unknown, options: EncodeOptions = {}): string {
  return Array.from(encodeLines(input, options)).join('\n')
}

/** Encodes normalized JSON as TOON lines without trailing newlines. */
export function encodeLines(input: unknown, options: EncodeOptions = {}): Iterable<string> {
  const delimiter = options.delimiter ?? ','
  if (![',', '|', '\t'].includes(delimiter)) throw new TypeError('invalid delimiter')
  const indentSize = options.indentSize ?? options.indent ?? 2
  const rawMaxDepth = options.maxDepth ?? DEFAULT_MAX_DEPTH
  const maxDepth = rawMaxDepth === Number.POSITIVE_INFINITY
    ? 0
    : Math.max(0, Math.floor(rawMaxDepth))
  const normalized = normalizeValue(input)
  const value = options.replacer
    ? applyReplacer(normalized, options.replacer)
    : normalized
  const resolved = {
    delimiter,
    indentSize,
    maxDepth,
    primitiveArrayColumns: options.primitiveArrayColumns === true,
    objectArrayColumns: options.objectArrayColumns === true,
  }
  if (options.cyclicDiscriminatedArrays === true) {
    const cyclic = cyclicDiscriminatedArrayWire(value)
    if (cyclic !== undefined) return cyclic.trimEnd().split('\n')
  }
  return encodeValue(value, resolved)
}

function encodeValue(value: any, options: ResolvedOptions): string[] {
  if (isPrimitive(value)) return [primitiveText(value, options.delimiter)]
  if (Array.isArray(value)) return encodeArray(undefined, value, 0, options)
  const fields = keyedFields(value, options)
  return fields === undefined
    ? encodeObject(value, 0, options)
    : encodeKeyed(undefined, value, fields, 0, options)
}

function encodeObject(value: Record<string, any>, depth: number, options: ResolvedOptions): string[] {
  checkDepth(depth, options)
  return Object.entries(value).flatMap(([key, nested]) => encodeField(key, nested, depth, options))
}

function encodeField(key: string, value: any, depth: number, options: ResolvedOptions): string[] {
  const prefix = indentation(depth, options) + canonicalKey(key)
  if (isPrimitive(value)) return [`${prefix}: ${primitiveText(value, options.delimiter)}`]
  if (Array.isArray(value)) return encodeArray(key, value, depth, options)

  const fields = keyedFields(value, options)
  if (fields !== undefined) return encodeKeyed(key, value, fields, depth, options)
  const lines = [`${prefix}:`]
  if (Object.keys(value).length > 0) lines.push(...encodeObject(value, depth + 1, options))
  return lines
}

function encodeKeyed(
  key: string | undefined,
  value: Record<string, any>,
  fields: FieldNode[],
  depth: number,
  options: ResolvedOptions,
): string[] {
  checkDepth(depth, options)
  const entries = Object.entries(value)
  checkFieldDepth(fields, depth + 1, options)
  const lines = [
    indentation(depth, options) + header(key, entries.length, fields, options.delimiter, true),
  ]
  for (const [entryKey, entryValue] of entries) {
    const row = encodeTabularRow(entryValue, fields, depth + 2, options)
    lines.push(indentation(depth + 1, options) + canonicalKey(entryKey) + ': ' + row.cells)
    lines.push(...row.children)
  }
  return lines
}

function encodeArray(
  key: string | undefined,
  value: any[],
  depth: number,
  options: ResolvedOptions,
): string[] {
  checkDepth(depth, options)
  const prefix = indentation(depth, options)
  if (value.length === 0) return [key === undefined ? `${prefix}[]` : `${prefix}${canonicalKey(key)}: []`]
  if (value.every(isPrimitive)) {
    return [
      prefix + header(key, value.length, undefined, options.delimiter) + ' ' + encodeCells(value, options.delimiter),
    ]
  }
  const fields = tabularFields(value, options)
  if (fields !== undefined) return encodeTabular(key, value, fields, depth, options)

  const lines = [prefix + header(key, value.length, undefined, options.delimiter)]
  for (const item of value) lines.push(...encodeListItem(item, depth + 1, options))
  return lines
}

function encodeTabular(
  key: string | undefined,
  rows: any[],
  fields: FieldNode[],
  depth: number,
  options: ResolvedOptions,
): string[] {
  checkFieldDepth(fields, depth + 1, options)
  const lines = [indentation(depth, options) + header(key, rows.length, fields, options.delimiter)]
  for (const row of rows) {
    const encoded = encodeTabularRow(row, fields, depth + 2, options)
    lines.push(indentation(depth + 1, options) + encoded.cells)
    lines.push(...encoded.children)
  }
  return lines
}

function encodeListItem(value: any, depth: number, options: ResolvedOptions): string[] {
  checkDepth(depth, options)
  const prefix = indentation(depth, options) + '-'
  if (isPrimitive(value)) return [`${prefix} ${primitiveText(value, options.delimiter)}`]
  if (Array.isArray(value)) {
    if (value.length === 0) return [`${prefix} ${header(undefined, 0, undefined, options.delimiter)}`]
    if (value.every(isPrimitive)) {
      return [`${prefix} ${header(undefined, value.length, undefined, options.delimiter)} ${encodeCells(value, options.delimiter)}`]
    }
    const lines = [`${prefix} ${header(undefined, value.length, undefined, options.delimiter)}`]
    for (const item of value) lines.push(...encodeListItem(item, depth + 1, options))
    return lines
  }
  return encodeObjectListItem(value, depth, options)
}

function encodeObjectListItem(
  value: Record<string, any>,
  depth: number,
  options: ResolvedOptions,
): string[] {
  checkDepth(depth, options)
  const entries = Object.entries(value)
  if (entries.length === 0) return [indentation(depth, options) + '-']

  const [[firstKey, firstValue], ...rest] = entries
  const special = encodeFirstContainer(firstKey, firstValue, depth, options)
  let lines: string[]
  if (special !== undefined) {
    lines = special
  } else if (isPrimitive(firstValue)) {
    lines = [
      indentation(depth, options) +
        '- ' +
        canonicalKey(firstKey) +
        ': ' +
        primitiveText(firstValue, options.delimiter),
    ]
  } else if (Array.isArray(firstValue)) {
    if (firstValue.length === 0) {
      lines = [indentation(depth, options) + '- ' + canonicalKey(firstKey) + ': []']
    } else {
      lines = [
        indentation(depth, options) +
          '- ' +
          header(firstKey, firstValue.length, undefined, options.delimiter),
      ]
      for (const item of firstValue) lines.push(...encodeListItem(item, depth + 2, options))
    }
  } else {
    lines = [indentation(depth, options) + '- ' + canonicalKey(firstKey) + ':']
    if (Object.keys(firstValue).length > 0) lines.push(...encodeObject(firstValue, depth + 2, options))
  }

  if (rest.length > 0) lines.push(...encodeObject(Object.fromEntries(rest), depth + 1, options))
  return lines
}

function encodeFirstContainer(
  key: string,
  value: any,
  depth: number,
  options: ResolvedOptions,
): string[] | undefined {
  if (Array.isArray(value) && value.length > 0 && value.every(isPrimitive)) {
    return [
      indentation(depth, options) +
        '- ' +
        header(key, value.length, undefined, options.delimiter) +
        ' ' +
        encodeCells(value, options.delimiter),
    ]
  }
  if (Array.isArray(value) && value.every(isPlainObject)) {
    const fields = tabularFields(value, options)
    if (fields !== undefined) {
      checkFieldDepth(fields, depth + 1, options)
      const lines = [
        indentation(depth, options) + '- ' + header(key, value.length, fields, options.delimiter),
      ]
      for (const row of value) {
        const encoded = encodeTabularRow(row, fields, depth + 3, options)
        lines.push(indentation(depth + 2, options) + encoded.cells)
        lines.push(...encoded.children)
      }
      return lines
    }
  }
  if (isPlainObject(value)) {
    const fields = keyedFields(value, options)
    if (fields !== undefined) {
      checkFieldDepth(fields, depth + 1, options)
      const entries = Object.entries(value)
      const lines = [
        indentation(depth, options) + '- ' + header(key, entries.length, fields, options.delimiter, true),
      ]
      for (const [entryKey, entryValue] of entries) {
        const encoded = encodeTabularRow(entryValue, fields, depth + 3, options)
        lines.push(indentation(depth + 2, options) + canonicalKey(entryKey) + ': ' + encoded.cells)
        lines.push(...encoded.children)
      }
      return lines
    }
  }
  return undefined
}

function header(
  key: string | undefined,
  length: number,
  fields: FieldNode[] | undefined,
  delimiter: string,
  keyed = false,
): string {
  const encodedKey = key === undefined ? '' : canonicalKey(key)
  const marker = keyed ? ':' : ''
  const delimiterMarker = delimiter === ',' ? '' : delimiter
  const fieldText = fields === undefined ? '' : `{${formatFields(fields, delimiter)}}`
  return `${encodedKey}[${length}${marker}${delimiterMarker}]${fieldText}:`
}

function formatFields(fields: FieldNode[], delimiter: string): string {
  return fields
    .map((field) => {
      const name = canonicalKey(field.name)
      if (field.listDelimiter !== undefined) return `${name}[${field.listDelimiter}]`
      if (field.fixedLength !== undefined) {
        const delimiterMarker = delimiter === ',' ? '' : delimiter
        return `${name}[${field.fixedLength}${delimiterMarker}]`
      }
      return name + (field.children === undefined ? '' : `{${formatFields(field.children, delimiter)}}`)
    })
    .join(delimiter)
}

function encodeTabularRow(
  value: any,
  fields: FieldNode[],
  childDepth: number,
  options: ResolvedOptions,
): { cells: string; children: string[] } {
  const cells: string[] = []
  const children: string[] = []

  for (const field of fields) {
    const nested = field.self === true ? value : value[field.name]
    if (field.childTable === true) {
      cells.push(String(nested.length))
      for (const child of nested) {
        const encoded = encodeTabularRow(child, field.children ?? [], childDepth + 1, options)
        children.push(indentation(childDepth, options) + encoded.cells, ...encoded.children)
      }
    } else if (field.fixedLength !== undefined) {
      cells.push(...nested.map((item) => primitiveText(item, options.delimiter)))
    } else if (field.listDelimiter !== undefined) {
      cells.push(
        nested
          .map((item) => primitiveListItemText(item, options.delimiter, field.listDelimiter))
          .join(field.listDelimiter),
      )
    } else if (field.children !== undefined) {
      const encoded = encodeTabularRow(nested, field.children, childDepth, options)
      cells.push(encoded.cells)
      children.push(...encoded.children)
    } else {
      cells.push(primitiveText(nested, options.delimiter))
    }
  }

  return { cells: cells.join(options.delimiter), children }
}

function primitiveListItemText(value: any, activeDelimiter: string, listDelimiter: string): string {
  if (typeof value !== 'string') return primitiveText(value, activeDelimiter)
  return needsQuotes(value, activeDelimiter) || value.includes(listDelimiter)
    ? quoteString(value)
    : value
}

function encodeCells(values: any[], delimiter: string): string {
  return values.map((value) => primitiveText(value, delimiter)).join(delimiter)
}

function indentation(depth: number, options: ResolvedOptions): string {
  return ' '.repeat(depth * options.indentSize)
}

function checkDepth(depth: number, options: ResolvedOptions): void {
  if (options.maxDepth !== 0 && depth > options.maxDepth) {
    throw toonError(0, `maximum nesting depth exceeded (maxDepth ${options.maxDepth})`)
  }
}

function checkFieldDepth(
  fields: FieldNode[],
  depth: number,
  options: ResolvedOptions,
): void {
  checkDepth(depth, options)
  for (const field of fields) {
    if (field.children !== undefined) checkFieldDepth(field.children, depth + 1, options)
  }
}
