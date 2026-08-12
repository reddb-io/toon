import { isPlainObject, isPrimitive } from './normalize.js'

export interface FieldNode {
  name: string
  children?: FieldNode[]
  childTable?: boolean
  fixedLength?: number
  listDelimiter?: ';'
  self?: boolean
}

export interface ShapeOptions {
  objectArrayColumns?: boolean
  primitiveArrayColumns?: boolean
}

/** Finds the recursive uniform shape required by v4.1 and extension tables. */
export function tabularFields(rows: any[], options: ShapeOptions = {}): FieldNode[] | undefined {
  if (options.objectArrayColumns === true) {
    const fixedLength = matrixLength(rows)
    if (fixedLength !== undefined) return [{ name: 'values', fixedLength, self: true }]
  }

  const firstKeys = Object.keys(rows[0] ?? {})
  if (firstKeys.length === 0) return undefined

  for (const row of rows) {
    if (!isPlainObject(row) || Object.keys(row).length !== firstKeys.length) return undefined
    if (firstKeys.some((key) => !Object.prototype.hasOwnProperty.call(row, key))) return undefined
  }

  const fields: FieldNode[] = []
  for (const name of firstKeys) {
    const values = rows.map((row) => row[name])
    if (values.every(isPrimitive)) {
      fields.push({ name })
      continue
    }
    if (
      options.primitiveArrayColumns === true &&
      values.every((value) => Array.isArray(value) && value.every(isPrimitive))
    ) {
      fields.push({ name, listDelimiter: ';' })
      continue
    }
    if (options.objectArrayColumns === true && values.every(Array.isArray)) {
      const fixedLength = matrixLength(values)
      if (fixedLength !== undefined) {
        fields.push({ name, fixedLength })
        continue
      }
      const childRows = values.flat()
      const children = tabularFields(childRows, options)
      if (children !== undefined) {
        fields.push({ name, children, childTable: true })
        continue
      }
    }
    if (!values.every((value) => isPlainObject(value) && Object.keys(value).length > 0)) return undefined
    const children = tabularFields(values, options)
    if (children === undefined) return undefined
    fields.push({ name, children })
  }
  return fields
}

/** Keyed form additionally requires at least two non-empty object entries. */
export function keyedFields(
  value: Record<string, any>,
  options: ShapeOptions = {},
): FieldNode[] | undefined {
  const rows = Object.values(value)
  if (rows.length < 2) return undefined
  if (!rows.every((row) => isPlainObject(row) && Object.keys(row).length > 0)) return undefined
  return tabularFields(rows, options)
}

function matrixLength(values: any[]): number | undefined {
  const firstLength = Array.isArray(values[0]) ? values[0].length : 0
  if (firstLength === 0) return undefined
  return values.every(
    (value) =>
      Array.isArray(value) &&
      value.length === firstLength &&
      value.every(isPrimitive),
  )
    ? firstLength
    : undefined
}
