export function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
