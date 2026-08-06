import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { decode, detectTruncation, encode, jsonToToon, toonToJson } from '../dist/index.js'

const cyclicCorpus = JSON.parse(
  readFileSync(
    new URL('../../../tests/corpus/wire-efficiency/cyclic-discriminated-arrays.json', import.meta.url),
    'utf8',
  ),
)
const childTableCorpus = JSON.parse(
  readFileSync(
    new URL('../../../tests/corpus/wire-efficiency/object-array-columns.json', import.meta.url),
    'utf8',
  ),
)

test('v4 codec cyclic discriminated arrays are opt-in and round-trip', () => {
  const fixture = cyclicCorpus.cases[0]
  const encoded = encode(fixture.expected, { cyclicDiscriminatedArrays: true })

  assert.equal(encoded, fixture.input.trimEnd())
  assert.notEqual(encode(fixture.expected), encoded)
  assert.deepEqual(decode(encoded), fixture.expected)
})

test('v4 codec child tables are opt-in, fail-closed, and round-trip', () => {
  for (const fixture of childTableCorpus.cases) {
    assert.throws(() => decode(fixture.input, { objectArrayColumns: false }), undefined, fixture.name)
    assert.deepEqual(decode(fixture.input), fixture.expected, fixture.name)
    const delimiter = fixture.input.split('\n', 1)[0].includes('|]') ? '|' : ','
    assert.deepEqual(
      decode(encode(fixture.expected, { objectArrayColumns: true, delimiter })),
      fixture.expected,
      fixture.name,
    )
  }

  for (const fixture of childTableCorpus.encodings) {
    const encoded = encode(fixture.value, { objectArrayColumns: true })
    assert.equal(encoded, fixture.expected.trimEnd(), fixture.name)
    assert.deepEqual(decode(encoded), fixture.value, fixture.name)
    assert.equal(encoded === encode(fixture.value), fixture.sameAsV3, fixture.name)
  }

  for (const fixture of childTableCorpus.errors) {
    assert.throws(
      () => decode(fixture.input),
      (error) => error?.line === fixture.line && error?.reason === fixture.reason,
      fixture.name,
    )
  }
})

test('truncation detection understands v4 comments and keyed tables', () => {
  assert.deepEqual(detectTruncation('# users\n[2:]{name}:\n  ada: Ada'), {
    complete: false,
    kind: 'array_length_mismatch',
    line: 3,
    declared: 2,
    actual: 1,
    message: 'declared 2 rows but received 1',
  })
})

test('TOONL whole-document bridges use the v4 codec', () => {
  const value = { people: { ada: { name: 'Ada' }, linus: { name: 'Linus' } } }
  const encoded = jsonToToon(JSON.stringify(value))

  assert.equal(encoded, 'people[2:]{name}:\n  ada: Ada\n  linus: Linus')
  assert.deepEqual(JSON.parse(toonToJson(`# generated\n${encoded}`)), value)
})
