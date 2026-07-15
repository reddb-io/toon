import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { parse, serialize } from '../src/index.js'

const REPO_ROOT = fileURLToPath(new URL('../../../', import.meta.url))
const FIXTURE_PATH = join(REPO_ROOT, 'tests/json-limits/corpus.json')
const EXPECTED_CASE_COUNT = 28
const REQUIRED_CATEGORIES = new Set([
  'numbers',
  'strings-unicode',
  'structure',
  'toon-decode',
  'toon-depth',
  'adversarial-round-trip',
])

function readFixture() {
  return JSON.parse(readFileSync(FIXTURE_PATH, 'utf8'))
}

function jsonEqual(left, right) {
  if (Array.isArray(left) || Array.isArray(right)) {
    return (
      Array.isArray(left) &&
      Array.isArray(right) &&
      left.length === right.length &&
      left.every((value, index) => jsonEqual(value, right[index]))
    )
  }
  if (left !== null && right !== null && typeof left === 'object' && typeof right === 'object') {
    const leftKeys = Object.keys(left)
    const rightKeys = Object.keys(right)
    return (
      leftKeys.length === rightKeys.length &&
      leftKeys.every(
        (key) =>
          Object.prototype.hasOwnProperty.call(right, key) && jsonEqual(left[key], right[key]),
      )
    )
  }
  return left === right
}

function expectedToon(expected) {
  return expected.toonJson === undefined ? expected.toon : JSON.parse(expected.toonJson)
}

test('JSON limits corpus resolves consistently for the JS package', () => {
  const fixture = readFixture()
  assert.equal(fixture.version, 'json-limits-v0.1')

  const categories = new Set()
  let executed = 0

  for (const testCase of fixture.tests) {
    const expected = testCase.expected.js
    categories.add(testCase.category)
    executed += 1

    if (testCase.rawToon !== undefined) {
      if (expected.error !== undefined) {
        assert.throws(
          () => parse(testCase.rawToon, testCase.parseOptions),
          (error) => error.message.includes(expected.error),
          `${testCase.name}: expected TOON decode error containing ${JSON.stringify(expected.error)}`,
        )
        continue
      }

      const actualRoundTrip = parse(testCase.rawToon, testCase.parseOptions)
      const expectedRoundTrip = JSON.parse(expected.roundTripJson)
      assert.ok(
        jsonEqual(actualRoundTrip, expectedRoundTrip),
        `${testCase.name}: expected TOON decode ${expected.roundTripJson}, got ${JSON.stringify(actualRoundTrip)}`,
      )
      continue
    }

    if (expected.error !== undefined) {
      assert.throws(
        () => serialize(JSON.parse(testCase.rawJson)),
        (error) => error.message.includes(expected.error),
        `${testCase.name}: expected error containing ${JSON.stringify(expected.error)}`,
      )
      continue
    }

    const value = JSON.parse(testCase.rawJson)
    const toon = serialize(value)
    assert.equal(toon, expectedToon(expected), `${testCase.name}: canonical TOON`)

    if (expected.nestedHeaderToon !== undefined) {
      const nestedToon = serialize(value, { nestedTabularHeaders: true })
      assert.equal(nestedToon, expected.nestedHeaderToon, `${testCase.name}: nested-header TOON`)
      assert.ok(
        jsonEqual(parse(nestedToon), JSON.parse(expected.roundTripJson)),
        `${testCase.name}: expected nested-header round trip ${expected.roundTripJson}, got ${JSON.stringify(parse(nestedToon))}`,
      )
    }

    const actualRoundTrip = parse(toon)
    const expectedRoundTrip = JSON.parse(expected.roundTripJson)
    assert.ok(
      jsonEqual(actualRoundTrip, expectedRoundTrip),
      `${testCase.name}: expected round trip ${expected.roundTripJson}, got ${JSON.stringify(actualRoundTrip)}`,
    )
  }

  assert.equal(executed, EXPECTED_CASE_COUNT, 'JSON limits case count changed')
  assert.deepEqual(categories, REQUIRED_CATEGORIES, 'all JSON limits categories are covered')
})
