const SURROGATE_PATTERN = /[\uD800-\uDFFF]/

/** Converts host values to the JSON data model before replacement and encoding. */
export function normalizeValue(value: unknown): any {
  if (value === null) return null

  if (
    typeof value === 'object' &&
    value !== null &&
    'toJSON' in value &&
    typeof (value as any).toJSON === 'function'
  ) {
    const next = (value as any).toJSON()
    if (next !== value) return normalizeValue(next)
  }

  if (typeof value === 'string') {
    assertNoLoneSurrogate(value, 'string value')
    return value
  }
  if (typeof value === 'boolean') return value
  if (typeof value === 'number') {
    if (Object.is(value, -0)) return 0
    return Number.isFinite(value) ? value : null
  }
  if (typeof value === 'bigint') {
    return value >= Number.MIN_SAFE_INTEGER && value <= Number.MAX_SAFE_INTEGER
      ? Number(value)
      : value.toString()
  }
  if (value instanceof Date) return value.toISOString()
  if (Array.isArray(value)) return value.map(normalizeValue)
  if (value instanceof Set) return Array.from(value, normalizeValue)
  if (value instanceof Map) {
    const result = {}
    for (const [key, nested] of value) setOwn(result, String(key), normalizeValue(nested))
    return result
  }
  if (isPlainObject(value)) {
    const result = {}
    for (const key of Object.keys(value)) {
      assertNoLoneSurrogate(key, 'object key')
      setOwn(result, key, normalizeValue(value[key]))
    }
    return result
  }
  return null
}

function assertNoLoneSurrogate(value: string, context: string): void {
  if (!SURROGATE_PATTERN.test(value)) return
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code < 0xd800 || code > 0xdfff) continue
    const next = value.charCodeAt(index + 1)
    if (code <= 0xdbff && next >= 0xdc00 && next <= 0xdfff) {
      index += 1
      continue
    }
    throw new TypeError(
      `Cannot encode ${context} containing an unpaired surrogate U+${code.toString(16).toUpperCase()} at index ${index}`,
    )
  }
}

export function isPlainObject(value: unknown): value is Record<string, any> {
  if (value === null || typeof value !== 'object') return false
  const prototype = Object.getPrototypeOf(value)
  return prototype === null || prototype === Object.prototype
}

export function isPrimitive(value: unknown): boolean {
  return value === null || ['string', 'number', 'boolean'].includes(typeof value)
}

export function setOwn(target: object, key: string, value: any): void {
  Object.defineProperty(target, key, {
    value,
    writable: true,
    enumerable: true,
    configurable: true,
  })
}
