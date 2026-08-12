import assert from 'node:assert/strict'
import test from 'node:test'

import { decode, encode } from '../dist/index.js'

test('decode canonicalizes every negative-zero spelling to positive zero', () => {
  const decoded = decode('values[3]: -0,-0.0,-0e2')

  assert.deepEqual(decoded, { values: [0, 0, 0] })
  assert.ok(decoded.values.every((value) => !Object.is(value, -0)))
})

test('replacer filters object properties and compacts array elements', () => {
  const input = {
    users: [
      { name: 'Ada', password: 'one', enabled: true },
      { name: 'Bob', password: 'two', enabled: false },
    ],
  }
  const replacer = (key, value) => {
    if (key === 'password') return undefined
    if (value?.enabled === false) return undefined
    return value
  }

  assert.deepEqual(decode(encode(input, { replacer })), {
    users: [{ name: 'Ada', enabled: true }],
  })
})

test('replacer transforms primitives and replacement containers recursively', () => {
  const replacer = (key, value, path) => {
    if (path.length === 1 && key === 'user') return { ...value, role: 'admin' }
    if (typeof value === 'string') return value.toUpperCase()
    return value
  }

  assert.deepEqual(decode(encode({ user: { name: 'Ada' } }, { replacer })), {
    user: { name: 'ADA', role: 'ADMIN' },
  })
})

test('replacer receives root, object, and array paths with JSON-style keys', () => {
  const calls = []
  encode({ rows: [{ value: 1 }] }, {
    replacer(key, value, path) {
      calls.push({ key, path: [...path] })
      return value
    },
  })

  assert.deepEqual(calls, [
    { key: '', path: [] },
    { key: 'rows', path: ['rows'] },
    { key: '0', path: ['rows', 0] },
    { key: 'value', path: ['rows', 0, 'value'] },
  ])
})

test('undefined from the root replacer preserves the root value', () => {
  const wire = encode({ name: 'Ada' }, {
    replacer(key, value, path) {
      return path.length === 0 ? undefined : value
    },
  })

  assert.deepEqual(decode(wire), { name: 'Ada' })
})

test('toJSON normalization runs before the replacer', () => {
  const seen = []
  const wire = encode({ created: new Date('2025-01-02T03:04:05.000Z') }, {
    replacer(key, value) {
      if (key === 'created') seen.push(value)
      return value
    },
  })

  assert.deepEqual(seen, ['2025-01-02T03:04:05.000Z'])
  assert.deepEqual(decode(wire), { created: '2025-01-02T03:04:05.000Z' })
})

test('replacer-created __proto__ properties remain ordinary own properties', () => {
  const replacement = {}
  Object.defineProperty(replacement, '__proto__', { value: 7, enumerable: true })
  const decoded = decode(encode({ payload: null }, {
    replacer(key, value) {
      return key === 'payload' ? replacement : value
    },
  }))

  assert.equal(Object.prototype.hasOwnProperty.call(decoded.payload, '__proto__'), true)
  assert.equal(decoded.payload.__proto__, 7)
})
