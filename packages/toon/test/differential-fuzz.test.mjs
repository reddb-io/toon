import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { test } from 'node:test'

import { decode as decodeLocal, encode as encodeLocal } from '../dist/index.js'
import {
  decode as decodeUpstream,
  encode as encodeUpstream,
} from '../../../vendor/toon/packages/toon/src/index.ts'
import { exerciseFixture, runDifferentialFuzz } from './support/differential-fuzz.mjs'

const implementations = {
  local: { decode: decodeLocal, encode: encodeLocal },
  upstream: { decode: decodeUpstream, encode: encodeUpstream },
}

test('committed differential counterexamples stay fixed', () => {
  const fixtureDirectory = new URL('./fixtures/differential/', import.meta.url)
  const fixtureNames = readdirSync(fixtureDirectory)
    .filter(name => name.endsWith('.json'))
    .sort()

  assert.ok(fixtureNames.length > 0, 'commit at least one minimized counterexample fixture')
  for (const name of fixtureNames) {
    const fixture = JSON.parse(readFileSync(new URL(name, fixtureDirectory), 'utf8'))
    exerciseFixture(fixture, implementations)
  }
})

test('local and vendored upstream codecs remain differential equivalents', () => {
  const result = runDifferentialFuzz(implementations, {
    cases: Number.parseInt(process.env.TOON_DIFFERENTIAL_CASES ?? '500', 10),
    seed: Number.parseInt(process.env.TOON_DIFFERENTIAL_SEED ?? '1592594824', 10),
    timeBudgetMs: Number.parseInt(process.env.TOON_DIFFERENTIAL_MS ?? '1500', 10),
  })

  assert.ok(result.casesRun > 0)
})
