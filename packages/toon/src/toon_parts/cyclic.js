import { toonError } from '../errors.js'
import { DOCUMENT_DELIMITER, canonicalKey, isPrimitive, primitiveText, setKey, splitDelimited } from '../lexical.js'
import { CYCLIC_DISCRIMINATOR_KEYS, CYCLIC_META_KEYS, CYCLIC_TABLE_DELIMITER } from './constants.js'
import { isPlainObject } from './common.js'
export function expandCyclicDiscriminatedArrays(document) {
  if (!isPlainObject(document) || Object.keys(document).length === 0) {
    return document
  }
  const expanded = {}
  for (const [key, value] of Object.entries(document)) {
    const section = cyclicArrayFromTabularObject(value, 1)
    if (section === undefined) {
      return document
    }
    setKey(expanded, key, section)
  }
  return expanded
}

function cyclicArrayFromTabularObject(section, line) {
  if (!isPlainObject(section)) {
    return undefined
  }
  if (!isCyclicSectionLike(section)) {
    return undefined
  }
  const { order, discriminator, rows } = section
  if (typeof order !== 'string' || typeof discriminator !== 'string' || !Number.isSafeInteger(rows) || rows < 0) {
    throw cyclicInvalid(line)
  }
  const labels = parseCyclicOrder(order, rows, line)
  const common = section.common
  const commonRows = common === undefined ? Array.from({ length: rows }, () => ({})) : common
  if (!Array.isArray(commonRows) || commonRows.length !== rows || !commonRows.every(isPlainObject)) {
    throw cyclicLengthMismatch(line)
  }

  const groups = new Map()
  for (const [label, table] of Object.entries(section)) {
    if (CYCLIC_META_KEYS.has(label)) {
      continue
    }
    if (!Array.isArray(table) || !table.every(isPlainObject)) {
      throw cyclicInvalid(line)
    }
    groups.set(label, table)
  }
  if (groups.size === 0) {
    throw cyclicInvalid(line)
  }

  const cursors = new Map()
  const values = labels.map((label, index) => {
    const group = groups.get(label)
    if (group === undefined) {
      throw cyclicGroupLengthMismatch(line)
    }
    const cursor = cursors.get(label) ?? 0
    const payload = group[cursor]
    if (payload === undefined) {
      throw cyclicGroupLengthMismatch(line)
    }
    cursors.set(label, cursor + 1)
    return mergeCyclicRow(discriminator, label, commonRows[index], payload, line)
  })
  for (const [label, group] of groups) {
    if ((cursors.get(label) ?? 0) !== group.length) {
      throw cyclicGroupLengthMismatch(line)
    }
  }
  return values
}

function isCyclicSectionLike(value) {
  return (
    Object.prototype.hasOwnProperty.call(value, 'order') ||
    Object.prototype.hasOwnProperty.call(value, 'discriminator') ||
    Object.prototype.hasOwnProperty.call(value, 'rows')
  )
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
  for (const [key, value] of Object.entries(common ?? {})) {
    if (key === discriminator || Object.prototype.hasOwnProperty.call(row, key)) {
      throw cyclicInvalid(line)
    }
    setKey(row, key, value)
  }
  for (const [key, value] of Object.entries(inflateCyclicFlatObject(payload, line))) {
    if (key === discriminator || Object.prototype.hasOwnProperty.call(row, key)) {
      throw cyclicInvalid(line)
    }
    setKey(row, key, value)
  }
  return row
}

function inflateCyclicFlatObject(value, line) {
  const nested = {}
  for (const [key, cell] of Object.entries(value)) {
    setCyclicPath(nested, key.split('.'), cell, line)
  }
  return inflateCyclicArrays(nested, line)
}

function setCyclicPath(target, path, value, line) {
  if (path.some((segment) => segment === '')) {
    throw cyclicInvalid(line)
  }
  let cursor = target
  for (let index = 0; index < path.length; index += 1) {
    const segment = path[index]
    if (index === path.length - 1) {
      cursor[segment] = value
      return
    }
    cursor[segment] ??= {}
    cursor = cursor[segment]
    if (!isPlainObject(cursor)) {
      throw cyclicInvalid(line)
    }
  }
}

function inflateCyclicArrays(value, line) {
  if (Array.isArray(value)) {
    return value.map((item) => inflateCyclicArrays(item, line))
  }
  if (!isPlainObject(value)) {
    return value
  }
  if (Number.isSafeInteger(value.length) && value.length >= 0) {
    return Array.from({ length: value.length }, (_, index) => {
      if (!Object.prototype.hasOwnProperty.call(value, String(index))) {
        throw cyclicInvalid(line)
      }
      return inflateCyclicArrays(value[String(index)], line)
    })
  }
  return Object.fromEntries(Object.entries(value).map(([key, nested]) => [key, inflateCyclicArrays(nested, line)]))
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

function percentEncode(value) {
  return encodeURIComponent(value)
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

export function cyclicDiscriminatedArrayWire(value) {
  if (!isPlainObject(value)) {
    return undefined
  }
  const entries = Object.entries(value)
  if (entries.length === 0) {
    return undefined
  }

  const sections = []
  for (const [key, rows] of entries) {
    const section = cyclicDiscriminatedArraySection(rows)
    if (section === undefined) {
      return undefined
    }
    sections.push([key, section])
  }

  const output = []
  for (const [key, section] of sections) {
    writeCyclicSection(output, key, section)
  }
  return output.join('')
}

function cyclicDiscriminatedArraySection(rows) {
  if (!Array.isArray(rows) || rows.length === 0 || !rows.every(isPlainObject)) {
    return undefined
  }
  const discriminator = cyclicDiscriminator(rows)
  if (discriminator === undefined) {
    return undefined
  }
  const labels = rows.map((row) => row[discriminator])
  const order = cyclicOrder(labels)
  if (order === undefined) {
    return undefined
  }
  const common = cyclicCommonPrefixKeys(rows, discriminator)
  const groups = new Map()
  for (const row of rows) {
    const label = row[discriminator]
    if (!groups.has(label)) {
      groups.set(label, [])
    }
    const payload = {}
    for (const [key, nested] of Object.entries(row)) {
      if (key !== discriminator && !common.includes(key)) {
        if (!flattenCyclicValue(nested, key, payload)) {
          return undefined
        }
      }
    }
    groups.get(label).push(payload)
  }
  const encodedGroups = []
  for (const label of order.cycle) {
    const payloads = groups.get(label)
    const fields = cyclicUniformFields(payloads)
    if (fields === undefined || fields.length === 0) {
      return undefined
    }
    encodedGroups.push({ fields, label, rows: payloads })
  }
  return {
    common,
    commonRows: rows.map((row) => Object.fromEntries(common.map((key) => [key, row[key]]))),
    discriminator,
    groups: encodedGroups,
    order,
    rows,
  }
}

function cyclicDiscriminator(rows) {
  return CYCLIC_DISCRIMINATOR_KEYS.find((key) => rows.every((row) => typeof row[key] === 'string'))
}

function cyclicOrder(labels) {
  if (labels.length < 12) {
    return undefined
  }
  for (let size = 2; size <= Math.min(8, Math.floor(labels.length / 3)); size += 1) {
    if (labels.length % size !== 0) {
      continue
    }
    const cycle = labels.slice(0, size)
    if (new Set(cycle).size < 2) {
      continue
    }
    if (!labels.every((label, index) => label === cycle[index % size])) {
      continue
    }
    const repeats = labels.length / size
    if (repeats < 3) {
      continue
    }
    const raw = labels.map(percentEncode).join(',')
    const encoded = `cycle(${cycle.map(percentEncode).join(',')})*${repeats}`
    if (encoded.length <= raw.length * 0.4) {
      return { cycle, encoded, repeats }
    }
  }
  return undefined
}

function cyclicCommonPrefixKeys(rows, discriminator) {
  const prefix = []
  const keys = Object.keys(rows[0])
  const start = keys.indexOf(discriminator) + 1
  for (const key of keys.slice(start).filter((candidate) => candidate !== discriminator)) {
    if (!isCyclicHeaderKey(key)) {
      break
    }
    if (!rows.every((row) => Object.prototype.hasOwnProperty.call(row, key))) {
      break
    }
    if (!rows.every((row) => isPrimitive(row[key]))) {
      break
    }
    prefix.push(key)
  }
  return prefix
}

function writeCyclicSection(output, key, section) {
  output.push(canonicalKey(key), ':\n')
  output.push('  order: ', primitiveText(section.order.encoded, CYCLIC_TABLE_DELIMITER), '\n')
  output.push('  discriminator: ', primitiveText(section.discriminator, CYCLIC_TABLE_DELIMITER), '\n')
  output.push('  rows: ', String(section.rows.length), '\n')
  if (section.common.length > 0) {
    writeCyclicTable(output, 'common', section.common, section.commonRows)
  }
  for (const group of section.groups) {
    writeCyclicTable(output, group.label, group.fields, group.rows)
  }
}

function writeCyclicTable(output, key, fields, rows) {
  output.push(
    '  ',
    canonicalKey(key),
    '[',
    String(rows.length),
    CYCLIC_TABLE_DELIMITER,
    ']{',
    fields.map(canonicalKey).join(CYCLIC_TABLE_DELIMITER),
    '}:\n',
  )
  for (const row of rows) {
    output.push(
      '    ',
      fields.map((field) => primitiveText(row[field], CYCLIC_TABLE_DELIMITER)).join(CYCLIC_TABLE_DELIMITER),
      '\n',
    )
  }
}

function isCyclicHeaderKey(value) {
  return value !== '' && isBareCyclicPath(value)
}

function flattenCyclicValue(value, prefix, out, seen = new Set()) {
  if (!isBareCyclicPath(prefix)) {
    return false
  }
  if (isPrimitive(value)) {
    out[prefix] = value
    return true
  }
  if (Array.isArray(value)) {
    if (seen.has(value)) {
      return false
    }
    seen.add(value)
    out[`${prefix}.length`] = value.length
    const valid = value.every((item, index) => flattenCyclicValue(item, `${prefix}.${index}`, out, seen))
    seen.delete(value)
    return valid
  }
  if (isPlainObject(value)) {
    if (seen.has(value)) {
      return false
    }
    seen.add(value)
    const valid = Object.entries(value).every(([key, nested]) => !key.includes('.') && flattenCyclicValue(nested, `${prefix}.${key}`, out, seen))
    seen.delete(value)
    return valid
  }
  return false
}

function cyclicUniformFields(rows) {
  const fields = Object.keys(rows[0] ?? {})
  if (!rows.every((row) => sameStringArray(Object.keys(row), fields))) {
    return undefined
  }
  return fields.every(isBareCyclicPath) ? fields : undefined
}

function sameStringArray(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index])
}

function isBareCyclicPath(value) {
  return value.split('.').every((segment) => /^[A-Za-z_][A-Za-z0-9_]*$|^(?:0|[1-9][0-9]*)$/.test(segment))
}
