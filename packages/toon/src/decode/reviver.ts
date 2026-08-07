import type { DecodeReviver, JsonArray, JsonObject, JsonValue } from '../types.js'
import { isPlainObject, normalizeValue, setOwn } from '../encode/normalize.js'

/** Apply an experimental decode reviver depth-first, from leaves to root. */
export function applyReviver(root: JsonValue, reviver: DecodeReviver): JsonValue {
  const transformed = transformChildren(root, reviver, [])
  const revivedRoot = reviver('', transformed, [])
  return revivedRoot === undefined ? transformed : normalizeValue(revivedRoot)
}

function transformChildren(
  value: JsonValue,
  reviver: DecodeReviver,
  path: readonly (string | number)[],
): JsonValue {
  if (Array.isArray(value)) return transformArray(value as JsonArray, reviver, path)
  if (isPlainObject(value)) return transformObject(value as JsonObject, reviver, path)
  return value
}

function transformObject(
  object: JsonObject,
  reviver: DecodeReviver,
  path: readonly (string | number)[],
): JsonObject {
  const result: JsonObject = {}
  for (const [key, value] of Object.entries(object)) {
    if (value === undefined) continue
    const childPath = [...path, key]
    const revived = reviver(key, transformChildren(value, reviver, childPath), childPath)
    if (revived !== undefined) setOwn(result, key, normalizeValue(revived))
  }
  return result
}

function transformArray(
  array: JsonArray,
  reviver: DecodeReviver,
  path: readonly (string | number)[],
): JsonArray {
  const result: JsonValue[] = []
  for (let index = 0; index < array.length; index += 1) {
    const childPath = [...path, index]
    const revived = reviver(
      String(index),
      transformChildren(array[index] as JsonValue, reviver, childPath),
      childPath,
    )
    if (revived !== undefined) result.push(normalizeValue(revived))
  }
  return result
}
