import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { parse, serialize } from '../src/index.js'

const REPO_ROOT = join(fileURLToPath(new URL('.', import.meta.url)), '..', '..', '..')
const FIXTURE_PATH = join(REPO_ROOT, 'tests/wire-efficiency/corpora.json')
const PRIMITIVE_ARRAY_COLUMNS_PATH = join(
  REPO_ROOT,
  'tests/wire-efficiency/primitive-array-columns.json',
)
const OBJECT_ARRAY_COLUMNS_PATH = join(
  REPO_ROOT,
  'tests/wire-efficiency/object-array-columns.json',
)
const EXT_OPTIONS = { nestedTabularHeaders: true, keyedMapCollapse: true, primitiveArrayColumns: true, objectArrayColumns: true }
const EXPECTED_CASE_COUNT = 9

function readFixture(path) {
  return JSON.parse(readFileSync(path, 'utf8'))
}

function byteLength(value) {
  return Buffer.byteLength(value, 'utf8')
}

test('wire-efficiency corpora assert encoded byte sizes for JS', () => {
  const fixture = readFixture(FIXTURE_PATH)
  assert.equal(fixture.seed, '0x5eed0096')
  assert.equal(fixture.cases.length, EXPECTED_CASE_COUNT, 'wire-efficiency case count changed')

  for (const testCase of fixture.cases) {
    const value = testCase.value
    const jsonMin = JSON.stringify(value)
    const toonV3 = serialize(value)
    const toonTab = serialize(value, { delimiter: '\t' })
    const toonExt = serialize(value, EXT_OPTIONS)

    assert.equal(byteLength(jsonMin), testCase.expectedBytes.jsonMin, `${testCase.name}: JSON min bytes`)
    assert.equal(byteLength(toonV3), testCase.expectedBytes.toonV3, `${testCase.name}: TOON v3 bytes`)
    assert.equal(byteLength(toonTab), testCase.expectedBytes.toonTab, `${testCase.name}: TOON tab bytes`)
    assert.equal(byteLength(toonExt), testCase.expectedBytes.toonExt, `${testCase.name}: TOON+ext bytes`)
    assert.deepEqual(parse(toonV3), value, `${testCase.name}: TOON v3 round trip`)
    assert.deepEqual(parse(toonTab), value, `${testCase.name}: TOON tab round trip`)
    assert.deepEqual(parse(toonExt), value, `${testCase.name}: TOON+ext round trip`)

    if (testCase.honestyZeroDelta) {
      assert.equal(toonExt, toonV3, `${testCase.name}: extensions must not change ineligible wire bytes`)
    }
  }
})

test('primitive-array column corpus decodes identically for JS', () => {
  const fixture = readFixture(PRIMITIVE_ARRAY_COLUMNS_PATH)
  assert.equal(fixture.version, 1)

  for (const testCase of fixture.cases) {
    assert.deepEqual(parse(testCase.input), testCase.expected, `${testCase.name}: decoded value`)
    if (testCase.failClosedV3Strict === true) {
      assert.throws(
        () => rejectV3Strict(testCase.input),
        /invalid array header/,
        `${testCase.name}: strict v3 rejects extension form`,
      )
    }
  }

  for (const testCase of fixture.errors) {
    assert.throws(
      () => parse(testCase.input),
      (error) =>
        error?.line === testCase.line &&
        error?.reason === testCase.reason &&
        error.message === `line ${testCase.line}: ${testCase.reason}`,
      `${testCase.name}: line-numbered parse error`,
    )
  }
})

test('object-array column corpus decodes identically for JS', () => {
  const fixture = readFixture(OBJECT_ARRAY_COLUMNS_PATH)
  assert.equal(fixture.version, 1)

  for (const testCase of fixture.cases) {
    assert.deepEqual(parse(testCase.input), testCase.expected, `${testCase.name}: decoded value`)
    if (testCase.failClosedV3Strict === true) {
      assert.throws(
        () => rejectV3Strict(testCase.input),
        /invalid array header/,
        `${testCase.name}: strict v3 rejects extension form`,
      )
    }
  }

  for (const testCase of fixture.errors) {
    assert.throws(
      () => parse(testCase.input),
      (error) =>
        error?.line === testCase.line &&
        error?.reason === testCase.reason &&
        error.message === `line ${testCase.line}: ${testCase.reason}`,
      `${testCase.name}: line-numbered parse error`,
    )
  }

  for (const testCase of fixture.encodings ?? []) {
    const options = jsOptions(testCase.options ?? {})
    const encoded = serialize(testCase.value, options)
    assert.equal(encoded, testCase.expected, `${testCase.name}: encoded wire`)
    assert.deepEqual(parse(encoded), testCase.value, `${testCase.name}: round trip`)
    if (testCase.sameAsV3 === true) {
      assert.equal(encoded, serialize(testCase.value), `${testCase.name}: v3.3 fallback`)
    } else {
      assert.notEqual(encoded, serialize(testCase.value), `${testCase.name}: extension wire`)
    }
    if (testCase.failClosedV3Strict === true) {
      assert.throws(
        () => rejectV3Strict(encoded),
        /invalid array header/,
        `${testCase.name}: strict v3 rejects extension form`,
      )
    }
  }
})

test('primitive-array column encoding is opt-in and falls back losslessly for ineligible values in JS', () => {
  const eligible = {
    items: [
      { id: 1, tags: ['hot', 'fragile'], note: 'a,b' },
      { id: 2, tags: ['semi;quoted'], note: 'plain' },
    ],
  }
  assert.equal(
    serialize(eligible, { primitiveArrayColumns: true }),
    'items[2]{id,tags[;],note}:\n  1,hot;fragile,"a,b"\n  2,"semi;quoted",plain\n',
  )
  assert.equal(serialize(eligible), 'items[2]:\n  - id: 1\n    tags[2]: hot,fragile\n    note: "a,b"\n  - id: 2\n    tags[1]: semi;quoted\n    note: plain\n')
  assert.deepEqual(parse(serialize(eligible, { primitiveArrayColumns: true })), eligible)

  const ineligible = { items: [{ id: 1, tags: null }, { id: 2, tags: ['ok'] }] }
  assert.equal(serialize(ineligible, { primitiveArrayColumns: true }), serialize(ineligible))
  assert.deepEqual(parse(serialize(ineligible, { primitiveArrayColumns: true })), ineligible)
})

test('object-array column encoding is opt-in and falls back losslessly for ineligible values in JS', () => {
  const eligible = {
    orders: [
      {
        id: 'ord_001',
        customer: 'cust_a',
        items: [
          { sku: 'sku_1', quantity: 3, components: [{ part: 'part_a', lot: 'lot_1', ok: true }] },
          { sku: 'sku_2', quantity: 1, components: [] },
        ],
      },
      { id: 'ord_002', customer: 'cust_b', items: [] },
    ],
  }
  const encoded = serialize(eligible, { objectArrayColumns: true, delimiter: '|' })
  assert.equal(
    encoded,
    'orders[2|]{id|customer|items{sku|quantity|components{part|lot|ok}}}:\n  ord_001|cust_a|2\n    sku_1|3|1\n      part_a|lot_1|true\n    sku_2|1|0\n  ord_002|cust_b|0\n',
  )
  assert.notEqual(encoded, serialize(eligible, { delimiter: '|' }))
  assert.deepEqual(parse(encoded), eligible)

  const matrix = { matrix: [[1, 2, 3], [4, 5, 6]] }
  const matrixEncoded = serialize(matrix, { objectArrayColumns: true, delimiter: '|' })
  assert.equal(matrixEncoded, 'matrix[2|]{values[3|]}:\n  1|2|3\n  4|5|6\n')
  assert.deepEqual(parse(matrixEncoded), matrix)

  const ineligible = { orders: [{ id: 'ord_001', items: [{ sku: 'a' }] }, { id: 'ord_002', items: [1] }] }
  assert.equal(serialize(ineligible, { objectArrayColumns: true }), serialize(ineligible))
  assert.deepEqual(parse(serialize(ineligible, { objectArrayColumns: true })), ineligible)
})

function rejectV3Strict(input) {
  input.split(/\n/).forEach((rawLine, index) => {
    const lineNumber = index + 1
    const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine
    if (/^[ ]*[^:[\n]+\[[0-9]+[|\t]?\]\{.*(?:\[[^\]]+\]|\{[^}]*\}).*\}:[ ]*$/.test(line)) {
      throw new Error(`line ${lineNumber}: invalid array header`)
    }
  })
}

function jsOptions(options) {
  const output = {}
  if (options.objectArrayColumns === true) {
    output.objectArrayColumns = true
  }
  if (options.primitiveArrayColumns === true) {
    output.primitiveArrayColumns = true
  }
  if (options.nestedTabularHeaders === true) {
    output.nestedTabularHeaders = true
  }
  if (options.keyedMapCollapse === true) {
    output.keyedMapCollapse = true
  }
  if (typeof options.delimiter === 'string') {
    output.delimiter = options.delimiter
  }
  return output
}
