import assert from 'node:assert/strict'
import test from 'node:test'

import { decode, decodeFromLines } from '../dist/index.js'

test('reviver visits decoded values bottom-up with property context', () => {
  const visits = []

  const result = decode('user:\n  names[2]: Ada,Linus', {
    reviver(key, value, path) {
      visits.push({ key, value: structuredClone(value), path })
      return value
    },
  })

  assert.deepEqual(result, { user: { names: ['Ada', 'Linus'] } })
  assert.deepEqual(visits, [
    { key: '0', value: 'Ada', path: ['user', 'names', 0] },
    { key: '1', value: 'Linus', path: ['user', 'names', 1] },
    { key: 'names', value: ['Ada', 'Linus'], path: ['user', 'names'] },
    { key: 'user', value: { names: ['Ada', 'Linus'] }, path: ['user'] },
    { key: '', value: { user: { names: ['Ada', 'Linus'] } }, path: [] },
  ])
})

test('reviver normalizes replacements and deletes object properties and array elements', () => {
  const result = decode('created: 2026-08-07\nsecret: hide\nvalues[4]: 1,2,3,4', {
    reviver(key, value) {
      if (key === 'created') return new Date(`${value}T00:00:00.000Z`)
      if (key === 'secret') return undefined
      if (typeof value === 'number' && value % 2 === 0) return undefined
      return value
    },
  })

  assert.deepEqual(result, {
    created: '2026-08-07T00:00:00.000Z',
    values: [1, 3],
  })
})

test('reviver can replace the root but cannot delete it', () => {
  assert.deepEqual(
    decode('name: Ada', {
      reviver(key, value) {
        return key === '' ? { wrapped: value } : value
      },
    }),
    { wrapped: { name: 'Ada' } },
  )

  assert.deepEqual(
    decode('name: Ada', {
      reviver(key, value) {
        return key === '' ? undefined : value
      },
    }),
    { name: 'Ada' },
  )
})

test('decode propagates errors thrown by the reviver unchanged', () => {
  const failure = new Error('reviver failed')
  assert.throws(
    () => decode('name: Ada', { reviver: () => { throw failure } }),
    (error) => error === failure,
  )
})

test('decode output is unchanged when no reviver is supplied', () => {
  assert.deepEqual(decode('user:\n  names[2]: Ada,Linus\nactive: true'), {
    user: { names: ['Ada', 'Linus'] },
    active: true,
  })
})

test('decodeFromLines exposes the same experimental reviver option', () => {
  assert.deepEqual(
    decodeFromLines(['name: Ada'], {
      reviver: (key, value) => key === 'name' ? 'Grace' : value,
    }),
    { name: 'Grace' },
  )
})
