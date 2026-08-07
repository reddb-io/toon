import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

import {
  DEFAULT_DELIMITER,
  DELIMITERS,
  ToonDecodeError,
  decode,
  decodeFromLines,
  encode,
  encodeLines,
  encodeToonlLines,
  escapeString,
  parse,
  rawString,
  serialize,
} from '../dist/index.js'
import { parse as parseLegacy, serialize as serializeLegacy } from '../dist/legacy.js'

test('the package root exposes the canonical v4.1 API and aliases', () => {
  assert.equal(parse, decode)
  assert.equal(serialize, encode)
  assert.deepEqual(decodeFromLines(['name: Ada', 'active: true']), {
    name: 'Ada',
    active: true,
  })
  assert.deepEqual([...encodeLines({ name: 'Ada', active: true })], [
    'name: Ada',
    'active: true',
  ])
  assert.deepEqual(DELIMITERS, { comma: ',', tab: '\t', pipe: '|' })
  assert.equal(DEFAULT_DELIMITER, ',')
})

test('raw strings and escaping compose at primitive positions', () => {
  const quoted = rawString(`"${escapeString('a"b')}"`)
  assert.equal(encode({ value: quoted }), 'value: "a\\"b"')
})

test('raw strings returned for containers leave traversal intact', () => {
  const output = encode({ name: 'Ada', age: 30 }, {
    replacer: (_key, value) => rawString(`"${escapeString(String(value))}"`),
  })
  assert.equal(output, 'name: "Ada"\nage: "30"')
})

test('decode failures expose stable positioned source and cause semantics', () => {
  assert.throws(
    () => decode('name: Ada\ngreeting: "hello'),
    (error) => {
      assert.ok(error instanceof ToonDecodeError)
      assert.ok(error instanceof SyntaxError)
      assert.equal(error.line, 2)
      assert.equal(error.source, 'greeting: "hello')
      assert.ok(error.cause instanceof Error)
      return true
    },
  )
})

test('legacy behavior is explicit and the TOONL emitter has its own name', () => {
  assert.equal(serializeLegacy({ value: 1 }), 'value: 1\n')
  assert.deepEqual(parseLegacy('a.b: 1\n', { expandPaths: 'safe' }), { a: { b: 1 } })

  const emitter = encodeToonlLines()
  assert.equal(emitter.push({ id: 1 }), '[]{id}:\n1\n')
})

test('generated declarations expose the canonical JSON, delimiter, event, and option types', () => {
  const declarations = readFileSync(new URL('../dist/index.d.ts', import.meta.url), 'utf8')
  for (const name of [
    'DecodeOptions',
    'Delimiter',
    'DelimiterKey',
    'EncodeOptions',
    'JsonArray',
    'JsonObject',
    'JsonPrimitive',
    'JsonStreamEvent',
    'JsonValue',
    'ResolvedDecodeOptions',
    'ResolvedEncodeOptions',
  ]) {
    assert.match(declarations, new RegExp(`\\b${name}\\b`))
  }
})
