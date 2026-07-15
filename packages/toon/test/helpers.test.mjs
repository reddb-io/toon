import test from 'node:test'
import assert from 'node:assert/strict'

import {
  appendSummaryField,
  decode,
  encode,
  parse,
  projectFields,
  serialize,
} from '../src/index.js'

test('appendSummaryField emits one conforming document with summary last', () => {
  const out = appendSummaryField({ service: 'checkout', rows: 3 }, { total: 3, failed: 1 })
  const back = parse(out)
  assert.deepEqual(back, { service: 'checkout', rows: 3, summary: { total: 3, failed: 1 } })
  const keys = Object.keys(back)
  assert.equal(keys[keys.length - 1], 'summary')
})

test('appendSummaryField replaces an existing summary key and moves it to the end', () => {
  const out = appendSummaryField({ summary: 'stale', a: 1 }, 'fresh')
  const back = parse(out)
  assert.deepEqual(back, { a: 1, summary: 'fresh' })
  assert.equal(Object.keys(back)[1], 'summary')
})

test('appendSummaryField output survives strings that need quoting', () => {
  const value = { note: 'a, b: [c] {d}\nnext' }
  const back = parse(appendSummaryField(value, 'ok'))
  assert.deepEqual(back, { note: 'a, b: [c] {d}\nnext', summary: 'ok' })
})

test('projectFields keeps allowlist order and drops other fields', () => {
  const rows = [
    { id: 1, state: 'active', noise: 'x' },
    { id: 2, state: 'merged', extra: true },
  ]
  const projected = projectFields(rows, ['state', 'id'])
  assert.deepEqual(projected, [
    { state: 'active', id: 1 },
    { state: 'merged', id: 2 },
  ])
  assert.deepEqual(Object.keys(projected[0]), ['state', 'id'])
})

test('projectFields leaves absent fields absent instead of null-filling', () => {
  const projected = projectFields([{ id: 1 }], ['id', 'missing'])
  assert.deepEqual(projected, [{ id: 1 }])
  assert.equal(Object.prototype.hasOwnProperty.call(projected[0], 'missing'), false)
})

test('encode/decode are exact aliases of serialize/parse', () => {
  assert.equal(encode, serialize)
  assert.equal(decode, parse)
})
