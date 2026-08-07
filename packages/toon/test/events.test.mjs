import assert from 'node:assert/strict'
import test from 'node:test'

import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

import { decodeFromLines, decodeStreamSync } from '../dist/index.js'

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

test('decodeFromLines emits before an async source produces its final line', async () => {
  let releaseFinalLine
  const finalLineReady = new Promise((resolve) => {
    releaseFinalLine = resolve
  })
  let finalLineProduced = false
  async function* lines() {
    yield 'name: Ada'
    yield 'active: true'
    await finalLineReady
    finalLineProduced = true
    yield 'role: admin'
  }

  const events = decodeFromLines(lines())
  const first = await events.next()

  assert.deepEqual(first, { done: false, value: { type: 'startObject', line: 1 } })
  assert.equal(finalLineProduced, false)
  releaseFinalLine()
  await events.return()
})

test('decodeFromLines does not complete a synchronous source before its first event', () => {
  function* lines() {
    yield 'name: Ada'
    yield 'active: true'
    throw new Error('final line was requested')
  }

  const events = decodeFromLines(lines())

  assert.deepEqual(events.next(), {
    done: false,
    value: { type: 'startObject', line: 1 },
  })
  events.return()
})
