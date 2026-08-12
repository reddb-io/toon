import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import {
  buildValueFromEvents,
  decode,
  decodeStreamSync,
  detectTruncation,
  encode,
  jsonToToon,
  ToonDecodeError,
  ToonError,
  toonToJson,
} from '../dist/index.js'

function decodeViaEvents(input, options) {
  return buildValueFromEvents(decodeStreamSync(input.split(/\r?\n/), options))
}

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
const primitiveArrayCorpus = JSON.parse(
  readFileSync(
    new URL('../../../tests/corpus/wire-efficiency/primitive-array-columns.json', import.meta.url),
    'utf8',
  ),
)
const truncationCorpus = JSON.parse(
  readFileSync(new URL('../../../tests/corpus/truncation.json', import.meta.url), 'utf8'),
)

test('canonical v4 encoders own extension emission', () => {
  const typescriptEncoder = readFileSync(
    new URL('../src/encode/serialize.ts', import.meta.url),
    'utf8',
  )
  const rustEncoder = readFileSync(
    new URL('../../../crates/toon/src/lib_parts/encode_v4.rs', import.meta.url),
    'utf8',
  )

  assert.doesNotMatch(typescriptEncoder, /toon_parts\/serialize|serializeLegacyExtensions/)
  assert.doesNotMatch(rustEncoder, /try_to_legacy_toon/)
})

test('v4 codec cyclic discriminated arrays reconstruct only with opt-in', () => {
  const fixture = cyclicCorpus.cases[0]
  const encoded = encode(fixture.expected, { cyclicDiscriminatedArrays: true })

  assert.equal(encoded, fixture.input.trimEnd())
  assert.notEqual(encode(fixture.expected), encoded)
  assert.deepEqual(decode(encoded), fixture.canonicalLiteral)
  assert.deepEqual(decode(encoded, { cyclicDiscriminatedArrays: true }), fixture.expected)
})

test('v4 codec primitive array columns use exact extension wire and decode the corpus', () => {
  for (const fixture of primitiveArrayCorpus.cases) {
    const encoded = encode(fixture.expected, {
      primitiveArrayColumns: true,
    })

    assert.equal(
      encoded,
      'items[3]{id,tags[;],quantity}:\n' +
        '  item_001,hazmat;oversize,60\n' +
        '  item_002,,0\n' +
        '  item_003,"frag;ile";null;true,7',
      fixture.name,
    )
    assert.notEqual(encode(fixture.expected), encoded, fixture.name)
    assert.deepEqual(decode(fixture.input), fixture.expected, fixture.name)
    assert.deepEqual(decodeViaEvents(fixture.input), fixture.expected, fixture.name)
  }

  for (const fixture of primitiveArrayCorpus.errors) {
    assert.throws(
      () => decode(fixture.input),
      (error) => error?.line === fixture.line && error?.reason === fixture.reason,
      fixture.name,
    )
    assert.throws(
      () => decodeViaEvents(fixture.input),
      (error) => error?.line === fixture.line && error?.reason === fixture.reason,
      fixture.name,
    )
  }
})

test('v4 codec child tables are opt-in, fail-closed, and round-trip', () => {
  for (const fixture of childTableCorpus.cases) {
    assert.throws(() => decode(fixture.input, { objectArrayColumns: false }), undefined, fixture.name)
    assert.throws(
      () => decodeViaEvents(fixture.input, { objectArrayColumns: false }),
      undefined,
      fixture.name,
    )
    assert.deepEqual(decode(fixture.input), fixture.expected, fixture.name)
    assert.deepEqual(decodeViaEvents(fixture.input), fixture.expected, fixture.name)
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
    assert.deepEqual(decodeViaEvents(encoded), fixture.value, fixture.name)
    assert.equal(encoded === encode(fixture.value), fixture.sameAsCanonical, fixture.name)
  }

  for (const fixture of childTableCorpus.errors) {
    assert.throws(
      () => decode(fixture.input),
      (error) => error?.line === fixture.line && error?.reason === fixture.reason,
      fixture.name,
    )
    assert.throws(
      () => decodeViaEvents(fixture.input),
      (error) => error?.line === fixture.line && error?.reason === fixture.reason,
      fixture.name,
    )
  }
})

test('upstream feedback tracks the mixed-columnar RFC without stale issue mappings', () => {
  const upstreamFeedback = readFileSync(
    new URL('../../../docs/upstream-feedback.md', import.meta.url),
    'utf8',
  )
  const migration = readFileSync(new URL('../../../docs/migration-v4.md', import.meta.url), 'utf8')
  const proposalIndex = readFileSync(
    new URL('../../../docs/proposals/README.md', import.meta.url),
    'utf8',
  )
  const delimiterProposal = readFileSync(
    new URL('../../../docs/proposals/delimiter-choice.md', import.meta.url),
    'utf8',
  )
  const primitiveProposal = readFileSync(
    new URL('../../../docs/proposals/primitive-array-columns.md', import.meta.url),
    'utf8',
  )

  assert.match(upstreamFeedback, /#48.*Mixed columnar arrays/i)
  assert.match(upstreamFeedback, /draft PR #47.*b5ce4c6/i)
  assert.match(upstreamFeedback, /Approval status:.*not approved.*not posted/i)
  assert.doesNotMatch(delimiterProposal, /toon-format\/spec\/issues\/48/)
  assert.doesNotMatch(primitiveProposal, /toon-format\/spec\/issues\/49/)
  assert.doesNotMatch(`${migration}\n${proposalIndex}`, /spec#48.*Delimiter choice/i)
  assert.doesNotMatch(`${migration}\n${proposalIndex}`, /spec#49.*Primitive-array columns/i)
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

  for (const fixture of truncationCorpus) {
    assert.deepEqual(
      detectTruncation(fixture.input, { format: fixture.format }),
      fixture.report,
      fixture.name,
    )
  }
})

test('v4 codec encode and decode enforce exact maximum-depth errors', () => {
  assert.throws(
    () => encode({ a: { b: { c: 1 } } }, { maxDepth: 1 }),
    (error) =>
      error instanceof ToonError &&
      error.line === 0 &&
      error.reason === 'maximum nesting depth exceeded (maxDepth 1)',
  )
  assert.throws(
    () => decode('a:\n  b:\n    c: 1', { maxDepth: 1 }),
    (error) =>
      error instanceof ToonDecodeError &&
      error.line === 3 &&
      error.reason === 'maximum nesting depth exceeded (maxDepth 1)',
  )

  assert.deepEqual(decode('a:\n  b:\n    c: 1', { maxDepth: 0 }), { a: { b: { c: 1 } } })
})

test('TOONL whole-document bridges use the v4 codec', () => {
  const value = { people: { ada: { name: 'Ada' }, linus: { name: 'Linus' } } }
  const encoded = jsonToToon(JSON.stringify(value))

  assert.equal(encoded, 'people[2:]{name}:\n  ada: Ada\n  linus: Linus')
  assert.deepEqual(JSON.parse(toonToJson(`# generated\n${encoded}`)), value)
})

test('object-array columns encode ragged nested object rows as a child table', () => {
  const value = {
    rows: [
      { id: 1, kids: [{ a: 1, b: 2 }, { a: 3, b: 4 }] },
      { id: 2, kids: [{ a: 5, b: 6 }] },
    ],
  }

  const encoded = encode(value, { objectArrayColumns: true })

  assert.equal(encoded, 'rows[2]{id,kids{a,b}}:\n  1,2\n    1,2\n    3,4\n  2,1\n    5,6')
  assert.deepEqual(decode(encoded, { objectArrayColumns: true }), value)
})

test('object-array columns encode equal-length primitive rows as fixed matrix columns', () => {
  const value = { rows: [{ id: 1, m: [1, 2] }, { id: 2, m: [3, 4] }] }

  const encoded = encode(value, { objectArrayColumns: true })

  assert.equal(encoded, 'rows[2]{id,m[2]}:\n  1,1,2\n  2,3,4')
  assert.deepEqual(decode(encoded, { objectArrayColumns: true }), value)
})
