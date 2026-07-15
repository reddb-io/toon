import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { parse, serialize } from '../src/index.js'

const REPO_ROOT = join(fileURLToPath(new URL('.', import.meta.url)), '..', '..', '..')
const FIXTURE_PATH = join(REPO_ROOT, 'tests/wire-efficiency/corpora.json')
const EXT_OPTIONS = { nestedTabularHeaders: true, keyedMapCollapse: true }
const EXPECTED_CASE_COUNT = 9

function readFixture() {
  return JSON.parse(readFileSync(FIXTURE_PATH, 'utf8'))
}

function byteLength(value) {
  return Buffer.byteLength(value, 'utf8')
}

test('wire-efficiency corpora assert encoded byte sizes for JS', () => {
  const fixture = readFixture()
  assert.equal(fixture.seed, '0x5eed0096')
  assert.equal(fixture.cases.length, EXPECTED_CASE_COUNT, 'wire-efficiency case count changed')

  for (const testCase of fixture.cases) {
    const value = testCase.value
    const jsonMin = JSON.stringify(value)
    const toonV3 = serialize(value)
    const toonExt = serialize(value, EXT_OPTIONS)

    assert.equal(byteLength(jsonMin), testCase.expectedBytes.jsonMin, `${testCase.name}: JSON min bytes`)
    assert.equal(byteLength(toonV3), testCase.expectedBytes.toonV3, `${testCase.name}: TOON v3 bytes`)
    assert.equal(byteLength(toonExt), testCase.expectedBytes.toonExt, `${testCase.name}: TOON+ext bytes`)
    assert.deepEqual(parse(toonV3), value, `${testCase.name}: TOON v3 round trip`)
    assert.deepEqual(parse(toonExt), value, `${testCase.name}: TOON+ext round trip`)

    if (testCase.honestyZeroDelta) {
      assert.equal(toonExt, toonV3, `${testCase.name}: extensions must not change ineligible wire bytes`)
    }
  }
})
