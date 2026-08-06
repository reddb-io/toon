/**
 * VERSION is a source constant, not a runtime read of package.json: the
 * package ships as plain ESM that may be bundled or vendored away from its
 * manifest, so reading the manifest at runtime is unreliable. That makes the
 * constant a lockstep declaration (ADR 0003) — scripts/sync-version.sh writes
 * it, and this test is what keeps it honest against the manifest.
 */

import { strict as assert } from 'node:assert'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

import { VERSION } from '../dist/index.js'

const manifest = JSON.parse(
  readFileSync(fileURLToPath(new URL('../package.json', import.meta.url)), 'utf8'),
)

test('VERSION matches the package manifest version', () => {
  assert.equal(VERSION, manifest.version)
})

test('VERSION is a plain semver string', () => {
  assert.equal(typeof VERSION, 'string')
  assert.match(VERSION, /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/)
})
