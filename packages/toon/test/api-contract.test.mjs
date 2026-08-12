import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

import * as rootApi from '../dist/index.js'
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
  rawString,
} from '../dist/index.js'

test('the package root exposes only the canonical v4.1 codec names', () => {
  assert.equal('parse' in rootApi, false)
  assert.equal('serialize' in rootApi, false)
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

test('indent options preserve upstream v4.1 edge semantics and alias precedence', () => {
  const nested = { a: { b: { c: 1 } } }

  assert.equal(encode(nested, { indentSize: 0 }), 'a:\nb:\nc: 1')
  assert.equal(encode(nested, { indent: 0 }), 'a:\nb:\nc: 1')
  assert.equal(encode(nested, { indentSize: 1.5 }), 'a:\n b:\n   c: 1')
  assert.equal(
    encode({ a: { b: 1 } }, { indentSize: 0, indent: 4 }),
    'a:\nb: 1',
  )
  assert.throws(() => encode({ a: { b: 1 } }, { indentSize: -1 }), RangeError)

  assert.deepEqual(
    decode('a:\n    b: 1', { indentSize: 4, indent: 2 }),
    { a: { b: 1 } },
  )
  assert.throws(() => decode('a: 1', { indentSize: 0 }), ToonDecodeError)
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

test('the TOONL emitter has its own name', () => {
  const emitter = encodeToonlLines()
  assert.equal(emitter.push({ id: 1 }), '[]{id}:\n1\n')
})

test('generated declarations expose the canonical JSON, delimiter, event, and option types', () => {
  const declarations = readFileSync(new URL('../dist/index.d.ts', import.meta.url), 'utf8')
  for (const name of [
    'DecodeOptions',
    'DecodeReviver',
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
  assert.doesNotMatch(declarations, /export declare const (?:parse|serialize)\b/)

  const optionDeclarations = readFileSync(new URL('../dist/types.d.ts', import.meta.url), 'utf8')
  assert.match(optionDeclarations, /reviver\?: DecodeReviver/)
})
