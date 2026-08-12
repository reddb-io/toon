import assert from 'node:assert/strict'
import test from 'node:test'

import {
  ToonDecodeError,
  ToonError,
  ToonlCursorInvalidationError,
  ToonlError,
  asToonlError,
} from '../dist/errors.js'
import { detectTruncation } from '../dist/decode/truncation.js'
import { normalizeValue } from '../dist/encode/normalize.js'
import { rawString } from '../dist/encode/raw-string.js'
import {
  cyclicDiscriminatedArrayWire,
  expandCyclicDiscriminatedArrays,
} from '../dist/cyclic.js'
import {
  ToonlEncoder,
  ToonlReader,
  encodeToonlLines,
  parseStream,
  recordTransform,
} from '../dist/toonl.js'

const throws = (operation) => assert.throws(operation, ToonError)

function cyclicSection(overrides = {}) {
  return {
    order: 'cycle(a,b)*1',
    discriminator: 'kind',
    rows: 2,
    a: [{ value: 1 }],
    b: [{ value: 2 }],
    ...overrides,
  }
}

const expands = (section) => expandCyclicDiscriminatedArrays({ items: section })

test('cyclic compatibility decoding validates metadata, groups and flattened paths', () => {
  assert.equal(expandCyclicDiscriminatedArrays(null), null)
  assert.deepEqual(expandCyclicDiscriminatedArrays({}), {})
  assert.deepEqual(expandCyclicDiscriminatedArrays({ items: [] }), { items: [] })
  assert.deepEqual(expands(cyclicSection()), {
    items: [
      { kind: 'a', value: 1 },
      { kind: 'b', value: 2 },
    ],
  })

  for (const section of [
    cyclicSection({ order: 1 }),
    cyclicSection({ common: [{}] }),
    cyclicSection({ a: [1] }),
    { order: 'cycle(a)*1', discriminator: 'kind', rows: 1 },
    cyclicSection({ order: 'cycle(a,c)*1' }),
    cyclicSection({ a: [] }),
    cyclicSection({ a: [{ value: 1 }, { value: 3 }] }),
    cyclicSection({ order: 'bad' }),
    cyclicSection({ order: 'cycle(a,b)' }),
    cyclicSection({ order: 'cycle()*2' }),
    cyclicSection({ order: 'cycle(a,b)*1+tail(a)' }),
    cyclicSection({ order: 'cycle(a,)*1' }),
    cyclicSection({ order: 'cycle(a,b)*01' }),
    cyclicSection({ order: 'cycle(a,b)*x' }),
    cyclicSection({ order: 'cycle(a,b)*9007199254740992' }),
    cyclicSection({ order: 'cycle(a,b)*2' }),
    cyclicSection({ order: 'cycle(%E0%A4%A,b)*1' }),
    cyclicSection({ common: [{ kind: 'wrong' }, {}] }),
    cyclicSection({ common: [{ value: 0 }, {}] }),
    cyclicSection({ a: [{ kind: 'wrong' }] }),
    cyclicSection({ a: [{ '': 1 }] }),
    cyclicSection({ a: [{ value: 1, 'value.deep': 2 }] }),
    cyclicSection({ a: [{ 'list.length': 2, 'list.1': 'only' }] }),
  ]) {
    throws(() => expands(section))
  }
})

const alternatingRows = (makeRow) =>
  Array.from({ length: 12 }, (_, index) => makeRow(index % 2 === 0 ? 'alpha' : 'beta', index))

test('cyclic compatibility encoding rejects ineligible shapes without partial wire', () => {
  assert.equal(cyclicDiscriminatedArrayWire(null), undefined)
  assert.equal(cyclicDiscriminatedArrayWire({}), undefined)
  assert.equal(cyclicDiscriminatedArrayWire({ items: [] }), undefined)
  assert.equal(cyclicDiscriminatedArrayWire({ items: [1] }), undefined)
  assert.equal(
    cyclicDiscriminatedArrayWire({ items: alternatingRows((_label, index) => ({ value: index })) }),
    undefined,
  )
  assert.equal(
    cyclicDiscriminatedArrayWire({ items: alternatingRows(() => ({ kind: 'same', value: 1 })) }),
    undefined,
  )
  assert.equal(
    cyclicDiscriminatedArrayWire({
      items: alternatingRows((kind, index) =>
        index === 0 ? { kind, value: 1 } : { kind, value: 1, extra: index },
      ),
    }),
    undefined,
  )
  const recursive = {}
  recursive.self = recursive
  assert.equal(
    cyclicDiscriminatedArrayWire({ items: alternatingRows((kind) => ({ kind, recursive })) }),
    undefined,
  )
  assert.equal(
    cyclicDiscriminatedArrayWire({ items: alternatingRows((kind) => ({ kind, 'bad.key': 1 })) }),
    undefined,
  )
})

test('normalization and error helpers cover non-JSON host values and causes', () => {
  assert.equal(normalizeValue(12n), 12)
  assert.equal(normalizeValue(9007199254740993n), '9007199254740993')
  assert.deepEqual(normalizeValue(new Set([1, 2])), [1, 2])
  assert.deepEqual(normalizeValue(new Map([['key', new Date('2020-01-01T00:00:00Z')]])), {
    key: '2020-01-01T00:00:00.000Z',
  })
  assert.equal(normalizeValue(Symbol('value')), null)
  assert.throws(() => normalizeValue('\ud800'), /unpaired surrogate/)
  assert.throws(() => normalizeValue({ '\udfff': 1 }), /unpaired surrogate/)
  assert.throws(() => rawString('# comment'), /line starting with/)
  assert.throws(() => rawString('value\n  # comment'), /line starting with/)

  const cause = new Error('cause')
  assert.equal(new ToonDecodeError('bad', { cause }).cause, cause)
  assert.equal(new ToonError(2, 'bad', { cause }).cause, cause)
  assert.equal(asToonlError(new ToonlError(1, 'bad')).line, 1)
  assert.equal(asToonlError(new ToonError(2, 'bad')).line, 2)
  assert.equal(asToonlError(new Error('plain')).reason, 'plain')
  assert.equal(asToonlError('plain').reason, 'plain')
  assert.equal(
    new ToonlCursorInvalidationError('inode_changed', 'moved', { path: 'data.toonl' }).condition,
    'inode_changed',
  )
})

test('truncation helpers cover validation-only branches', () => {
  assert.throws(() => detectTruncation('', { format: 'bad' }), TypeError)
  assert.equal(detectTruncation('items[3]: 1,2').kind, 'array_length_mismatch')
  assert.equal(detectTruncation('items[2]: 1,2\ninvalid line').kind, 'invalid')
  assert.equal(detectTruncation('items[3|]: 1|2').kind, 'array_length_mismatch')
  assert.equal(detectTruncation('items[2]{id}:\n  1').kind, 'array_length_mismatch')
  assert.equal(detectTruncation('items[2|]: 1|2').complete, true)
  assert.equal(detectTruncation('items[x]: nope').kind, 'invalid')
})

test('TOONL grammar reports malformed headers, rows, tags and continuations', () => {
  for (const input of [
    '[=x]',
    '[bad',
    '[;]{a}:',
    '[]<bad{a}:',
    '[|]<tag>{a}:',
    '[]{a}',
    '[]{,}:',
    '[]{"bad}:',
    '[~]{a}:',
    '[]{a}:\n[~|]{a}:',
    '[]{a}:\n[~]{b}:',
    '[]{a,b}:\n1',
    '[]{a}:\n"bad',
    '[=1]',
    '[]{a}:\n1\n[=2]',
    '- reserved',
    '[]<bad!>{a}:',
  ]) {
    assert.throws(() => parseStream(input), ToonlError, input)
  }
})

test('TOONL cursor and encoder validation is stable at public boundaries', async () => {
  const cursor = { activeHeaderLine: '[]{a}:', byteOffset: 0, rowsSinceHeader: 0 }
  for (const invalid of [
    { ...cursor, byteOffset: -1 },
    { ...cursor, rowsSinceHeader: -1 },
    { ...cursor, activeHeaderLine: '' },
    { ...cursor, activeHeaderLine: '[~]{a}:' },
    { ...cursor, activeHeaderLine: '[]<tag>{a}:' },
  ]) {
    assert.throws(() => new ToonlReader('', { cursor: invalid }), ToonlError)
  }

  const asyncSource = {
    async *[Symbol.asyncIterator]() {
      yield '[]{a}:\n1\n'
    },
  }
  await assert.rejects(async () => {
    for await (const _record of new ToonlReader(asyncSource, { cursor })) {
      // Cursor validation happens before the first record.
    }
  }, ToonlError)

  assert.throws(() => new ToonlEncoder(';', ['a']), ToonlError)
  assert.throws(() => new ToonlEncoder(',', []), ToonlError)
  assert.throws(() => new ToonlEncoder(',', ['']), ToonlError)
  assert.throws(() => new ToonlEncoder(',', ['"bad']), ToonlError)
  assert.throws(() => new ToonlEncoder(',', ['a'], { continuationEveryRows: 0 }), ToonlError)

  const encoder = new ToonlEncoder(',', ['a'])
  assert.deepEqual(encoder.fields, ['a'])
  assert.throws(() => encoder.setContinuationEveryRows(0), ToonlError)
  assert.throws(() => encoder.setContinuationEveryBytes(-1), ToonlError)
  assert.throws(() => encoder.pushRawRow([]), ToonlError)
  assert.throws(() => encoder.pushRawRow(['"bad']), ToonlError)
  assert.throws(() => encoder.pushRow({}), ToonlError)
  assert.throws(() => encoder.pushRow({ a: [] }), ToonlError)

  const emitter = encodeToonlLines()
  assert.throws(() => emitter.push(null), ToonlError)
  assert.throws(() => emitter.push({}), ToonlError)
  assert.throws(() => emitter.push({ nested: {} }), ToonlError)
  emitter.push({ id: 1 })
  emitter.end()
  assert.throws(() => emitter.push({ id: 2 }), ToonlError)
  assert.throws(() => emitter.declareLane('tag', ['id']), ToonlError)
  assert.throws(() => emitter.pushTagged('tag', { id: 2 }), ToonlError)

  assert.throws(() => recordTransform(null), ToonlError)
  const transformStream = globalThis.TransformStream
  try {
    globalThis.TransformStream = undefined
    assert.throws(() => recordTransform((record) => record), ToonlError)
  } finally {
    globalThis.TransformStream = transformStream
  }
})
