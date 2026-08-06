import { canonicalKey, primitiveText } from '../lexical.js'
import { isPlainObject, isPrimitive, normalizeValue } from './normalize.js'
import { applyReplacer, type EncodeReplacer } from './replacer.js'
import { collectLeaves, keyedFields, tabularFields, type FieldNode } from './shape.js'

export interface EncodeOptions {
  delimiter?: ',' | '|' | '\t'
  indentSize?: number
  /** @deprecated Use indentSize. */
  indent?: number
  replacer?: EncodeReplacer
}

interface ResolvedOptions {
  delimiter: ',' | '|' | '\t'
  indentSize: number
}

/** Encodes normalized JSON using the canonical v4.1 forms. */
export function encode(input: unknown, options: EncodeOptions = {}): string {
  const delimiter = options.delimiter ?? ','
  if (![',', '|', '\t'].includes(delimiter)) throw new TypeError('invalid delimiter')
  const indentSize = Math.max(1, Math.floor(options.indentSize ?? options.indent ?? 2))
  const normalized = normalizeValue(input)
  const value = options.replacer
    ? applyReplacer(normalized, options.replacer)
    : normalized
  return encodeValue(value, { delimiter, indentSize }).join('\n')
}

function encodeValue(value: any, options: ResolvedOptions): string[] {
  if (isPrimitive(value)) return [primitiveText(value, options.delimiter)]
  if (Array.isArray(value)) return encodeArray(undefined, value, 0, options)
  const fields = keyedFields(value)
  return fields === undefined
    ? encodeObject(value, 0, options)
    : encodeKeyed(undefined, value, fields, 0, options)
}

function encodeObject(value: Record<string, any>, depth: number, options: ResolvedOptions): string[] {
  return Object.entries(value).flatMap(([key, nested]) => encodeField(key, nested, depth, options))
}

function encodeField(key: string, value: any, depth: number, options: ResolvedOptions): string[] {
  const prefix = indentation(depth, options) + canonicalKey(key)
  if (isPrimitive(value)) return [`${prefix}: ${primitiveText(value, options.delimiter)}`]
  if (Array.isArray(value)) return encodeArray(key, value, depth, options)

  const fields = keyedFields(value)
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
  const entries = Object.entries(value)
  const lines = [
    indentation(depth, options) + header(key, entries.length, fields, options.delimiter, true),
  ]
  for (const [entryKey, entryValue] of entries) {
    lines.push(
      indentation(depth + 1, options) +
        canonicalKey(entryKey) +
        ': ' +
        encodeCells(collectLeaves(entryValue, fields), options.delimiter),
    )
  }
  return lines
}

function encodeArray(
  key: string | undefined,
  value: any[],
  depth: number,
  options: ResolvedOptions,
): string[] {
  const prefix = indentation(depth, options)
  if (value.length === 0) return [key === undefined ? `${prefix}[]` : `${prefix}${canonicalKey(key)}: []`]
  if (value.every(isPrimitive)) {
    return [
      prefix + header(key, value.length, undefined, options.delimiter) + ' ' + encodeCells(value, options.delimiter),
    ]
  }
  if (value.every(isPlainObject)) {
    const fields = tabularFields(value)
    if (fields !== undefined) return encodeTabular(key, value, fields, depth, options)
  }

  const lines = [prefix + header(key, value.length, undefined, options.delimiter)]
  for (const item of value) lines.push(...encodeListItem(item, depth + 1, options))
  return lines
}

function encodeTabular(
  key: string | undefined,
  rows: Record<string, any>[],
  fields: FieldNode[],
  depth: number,
  options: ResolvedOptions,
): string[] {
  const lines = [indentation(depth, options) + header(key, rows.length, fields, options.delimiter)]
  for (const row of rows) {
    lines.push(indentation(depth + 1, options) + encodeCells(collectLeaves(row, fields), options.delimiter))
  }
  return lines
}

function encodeListItem(value: any, depth: number, options: ResolvedOptions): string[] {
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
    const fields = tabularFields(value)
    if (fields !== undefined) {
      const lines = [
        indentation(depth, options) + '- ' + header(key, value.length, fields, options.delimiter),
      ]
      for (const row of value) {
        lines.push(indentation(depth + 2, options) + encodeCells(collectLeaves(row, fields), options.delimiter))
      }
      return lines
    }
  }
  if (isPlainObject(value)) {
    const fields = keyedFields(value)
    if (fields !== undefined) {
      const entries = Object.entries(value)
      const lines = [
        indentation(depth, options) + '- ' + header(key, entries.length, fields, options.delimiter, true),
      ]
      for (const [entryKey, entryValue] of entries) {
        lines.push(
          indentation(depth + 2, options) +
            canonicalKey(entryKey) +
            ': ' +
            encodeCells(collectLeaves(entryValue, fields), options.delimiter),
        )
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
    .map((field) =>
      canonicalKey(field.name) +
      (field.children === undefined ? '' : `{${formatFields(field.children, delimiter)}}`),
    )
    .join(delimiter)
}

function encodeCells(values: any[], delimiter: string): string {
  return values.map((value) => primitiveText(value, delimiter)).join(delimiter)
}

function indentation(depth: number, options: ResolvedOptions): string {
  return ' '.repeat(depth * options.indentSize)
}
