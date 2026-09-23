import { toonError } from '../errors.js'
import { setKey } from '../lexical.js'
import { isRawString } from './raw-string.js'

const SURROGATE_PATTERN = /[\uD800-\uDFFF]/

/**
 * Containers shallower than this skip cycle tracking: a cycle repeats, so it
 * always reaches this depth and is caught there, while ordinary documents never
 * pay for the WeakSet.
 */
const CYCLE_TRACKING_DEPTH = 32

interface NormalizeContext {
  /** 0 disables the guard. */
  maxDepth: number
  /** Containers on the current path; shared (acyclic) references stay legal. */
  active: WeakSet<object>
}

/** Converts host values to the JSON data model before replacement and encoding. */
export function normalizeValue(value: unknown, maxDepth = 0): any {
  return normalizeNested(value, { maxDepth, active: new WeakSet() }, 0)
}

function normalizeNested(value: unknown, context: NormalizeContext, depth: number): any {
  if (value === null) return null
  if (isRawString(value)) return value

  if (
    typeof value === 'object' &&
    value !== null &&
    'toJSON' in value &&
    typeof (value as any).toJSON === 'function'
  ) {
    const next = (value as any).toJSON()
    if (next !== value) return normalizeNested(next, context, depth)
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
  if (Array.isArray(value) || value instanceof Set || value instanceof Map || isPlainObject(value)) {
    return normalizeContainer(value, context, depth)
  }
  return null
}

function normalizeContainer(value: object, context: NormalizeContext, depth: number): any {
  // The serializer enforces maxDepth exactly; this guard only stops a runaway
  // recursion one level past it, before the host stack overflows.
  if (context.maxDepth !== 0 && depth > context.maxDepth + 1) {
    throw toonError(0, `maximum nesting depth exceeded (maxDepth ${context.maxDepth})`)
  }
  if (depth < CYCLE_TRACKING_DEPTH) return normalizeChildren(value, context, depth)
  if (context.active.has(value)) throw new TypeError('Cannot encode a circular structure')
  context.active.add(value)
  try {
    return normalizeChildren(value, context, depth)
  } finally {
    context.active.delete(value)
  }
}

function normalizeChildren(value: object, context: NormalizeContext, depth: number): any {
  const child = (nested: unknown) => normalizeNested(nested, context, depth + 1)
  // Array.from visits holes, so a sparse slot normalizes to null like undefined.
  if (Array.isArray(value) || value instanceof Set) return Array.from(value, child)
  const result = {}
  if (value instanceof Map) {
    for (const [key, nested] of value) setKey(result, String(key), child(nested))
    return result
  }
  for (const key of Object.keys(value)) {
    assertNoLoneSurrogate(key, 'object key')
    setKey(result, key, child(value[key]))
  }
  return result
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
  return value === null || isRawString(value) || ['string', 'number', 'boolean'].includes(typeof value)
}

export function setOwn(target: object, key: string, value: any): void {
  setKey(target, key, value)
}
