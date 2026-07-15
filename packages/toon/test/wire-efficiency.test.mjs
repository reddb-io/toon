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
const EXT_OPTIONS = { nestedTabularHeaders: true, keyedMapCollapse: true }
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

function rejectV3Strict(input) {
  input.split(/\n/).forEach((rawLine, index) => {
    const lineNumber = index + 1
    const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine
    if (/^[ ]*[^:[\n]+\[[0-9]+[|\t]?\]\{.*\[[^\]]+\].*\}:[ ]*$/.test(line)) {
      throw new Error(`line ${lineNumber}: invalid array header`)
    }
  })
}
