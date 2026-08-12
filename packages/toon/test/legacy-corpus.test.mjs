import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import test from 'node:test'

import { ToonError } from '../dist/errors.js'
import { parse } from '../dist/legacy.js'

const fixtureRoot = new URL('../../../vendor/toon-spec/tests/fixtures/decode/', import.meta.url)

function decoderOptions(options = {}) {
  return {
    expandPaths: options.expandPaths === 'safe',
    indent: options.indent ?? options.indentSize,
    strict: options.strict,
  }
}

test('the legacy decoder accepts or structurally rejects the full v4 corpus', () => {
  let executed = 0

  for (const file of readdirSync(fixtureRoot).filter((path) => path.endsWith('.json')).sort()) {
    const fixture = JSON.parse(readFileSync(join(fixtureRoot.pathname, file), 'utf8'))
    for (const testCase of fixture.tests) {
      executed += 1
      try {
        const value = parse(testCase.input, decoderOptions(testCase.options))
        assert.doesNotThrow(() => JSON.stringify(value), `${file}: ${testCase.name}`)
      } catch (error) {
        assert.ok(error instanceof ToonError, `${file}: ${testCase.name}`)
        assert.ok(Number.isInteger(error.line), `${file}: ${testCase.name}`)
        assert.ok(error.reason.length > 0, `${file}: ${testCase.name}`)
      }
    }
  }

  assert.ok(executed >= 200, `expected the full decode corpus, ran only ${executed} cases`)
})
