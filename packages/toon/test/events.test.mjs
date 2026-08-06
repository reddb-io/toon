import assert from 'node:assert/strict'
import test from 'node:test'

import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

import { decodeStreamSync } from '../dist/decode/stream.js'

const here = dirname(fileURLToPath(import.meta.url))
const fixturesDir = join(here, '..', '..', '..', 'tests', 'corpus', 'events')

for (const file of readdirSync(fixturesDir).filter((name) => name.endsWith('.json'))) {
  const cases = JSON.parse(readFileSync(join(fixturesDir, file), 'utf8'))
  for (const fixture of cases) {
    test(`events/${file}: ${fixture.name}`, () => {
      const lines = fixture.input.split('\n')
      const options = { strict: fixture.strict ?? true }
      const emitted = []
      let failure
      try {
        for (const event of decodeStreamSync(lines, options)) emitted.push(event)
      } catch (error) {
        failure = error
      }

      assert.deepEqual(emitted, fixture.events)
      if (fixture.error) {
        assert.ok(failure, 'expected the decoder to fail, but it completed')
        assert.equal(failure.line, fixture.error.line)
      } else if (failure) {
        throw failure
      }
    })
  }
}
