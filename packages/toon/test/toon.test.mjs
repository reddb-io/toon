import assert from 'node:assert/strict'
import test from 'node:test'

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import {
  ToonDecodeError,
  ToonError,
  decode as parse,
  detectTruncation,
  encode as serialize,
} from '../dist/index.js'

/** `assert.throws` returns nothing, so capture the error to inspect its line. */
function caught(fn) {
  try {
    fn()
  } catch (error) {
    return error
  }
  return assert.fail('expected a throw')
}

function deeplyNestedToon(depth) {
  return Array.from({ length: depth + 1 }, (_, index) => `${'  '.repeat(index)}k${index}:`).join('\n') + '\n'
}

function deeplyNestedObject(depth) {
  let value = 'leaf'
  for (let index = depth; index >= 0; index -= 1) {
    value = { [`k${index}`]: value }
  }
  return value
}

const truncationCorpus = JSON.parse(
  readFileSync(fileURLToPath(new URL('../../../tests/corpus/truncation.json', import.meta.url)), 'utf8'),
)

test('parses flat fields and serializes canonical TOON', () => {
  const document = parse('name : Ada\nactive: true\ncount: 3\n')

  assert.deepEqual(document, { name: 'Ada', active: true, count: 3 })
  assert.equal(serialize(document), 'name: Ada\nactive: true\ncount: 3')
})

test('detects truncation with the shared structured report corpus', () => {
  for (const fixture of truncationCorpus) {
    assert.deepEqual(
      detectTruncation(fixture.input, { format: fixture.format }),
      fixture.report,
      fixture.name,
    )
  }
})

test('parses nested objects', () => {
  const document = parse('person:\n  address:\n    city: London\n')

  assert.deepEqual(document, { person: { address: { city: 'London' } } })
})

test('parses tabular arrays and serializes canonical TOON', () => {
  const document = parse('users[2]{id,name,active}:\n  1,Ada,true\n  2,"Bob Smith",false\n')

  assert.deepEqual(document, {
    users: [
      { id: 1, name: 'Ada', active: true },
      { id: 2, name: 'Bob Smith', active: false },
    ],
  })
  assert.equal(
    serialize(document),
    'users[2]{id,name,active}:\n  1,Ada,true\n  2,Bob Smith,false',
  )
})

test('parses nested tabular headers', () => {
  const document = parse(
    'orders[2]{id,customer{name,country},total}:\n  1,Ada,UK,10.5\n  2,Bob,US,20\n',
  )

  assert.deepEqual(document, {
    orders: [
      { id: 1, customer: { name: 'Ada', country: 'UK' }, total: 10.5 },
      { id: 2, customer: { name: 'Bob', country: 'US' }, total: 20 },
    ],
  })
})

test('serializes nested tabular headers as the canonical form', () => {
  const document = {
    orders: [
      { id: 1, customer: { name: 'Ada', country: 'UK' }, total: 10.5 },
      { id: 2, customer: { name: 'Bob', country: 'US' }, total: 20 },
    ],
  }
  const nested = 'orders[2]{id,customer{name,country},total}:\n  1,Ada,UK,10.5\n  2,Bob,US,20'

  assert.equal(serialize(document), nested)
  assert.deepEqual(parse(nested), document)
})

test('nested tabular serialization falls back on recursive shape mismatch', () => {
  const document = {
    rows: [
      { id: 1, point: { x: 1, y: 2 } },
      { id: 2, point: { x: 3, z: 4 } },
    ],
  }

  assert.equal(
    serialize(document),
    'rows[2]:\n  - id: 1\n    point:\n      x: 1\n      y: 2\n  - id: 2\n    point:\n      x: 3\n      z: 4',
  )
})

test('nested tabular headers validate leaf arity and shape', () => {
  assert.throws(
    () => parse('orders[1]{id,customer{name,country}}:\n  1,Ada\n'),
    (error) => error.line === 2 && /array row length mismatch/.test(error.message),
  )
  assert.throws(() => parse('orders[1]{id,customer{}}:\n  1\n'), /invalid array header/)
  assert.throws(
    () => parse('orders[1]{customer{name},customer{name}}:\n  Ada,Bob\n'),
    /duplicate field name in header/,
  )
  assert.throws(() => parse('orders[1]{id,customer{name,country}:\n  1,Ada,UK\n'), {
    message: /malformed tabular header fields/,
  })
})

test('parses keyed map rows', () => {
  const input =
    'people[2:]{first,last,meta{active,score}}:\n  joe: Joe,Schmoe,true,7\n  mary: Mary,Jane,false,9\n'

  assert.deepEqual(parse(input), {
    people: {
      joe: { first: 'Joe', last: 'Schmoe', meta: { active: true, score: 7 } },
      mary: { first: 'Mary', last: 'Jane', meta: { active: false, score: 9 } },
    },
  })
})

test('serializes a uniform object map as a keyed table', () => {
  const document = {
    people: {
      joe: { first: 'Joe', last: 'Schmoe' },
      mary: { first: 'Mary', last: 'Jane' },
    },
  }
  const keyed = 'people[2:]{first,last}:\n  joe: Joe,Schmoe\n  mary: Mary,Jane'

  assert.equal(serialize(document), keyed)
  assert.deepEqual(parse(keyed), document)
})

test('the keyed table form falls back for non-uniform maps', () => {
  const document = {
    people: {
      joe: { first: 'Joe', last: 'Schmoe' },
      mary: { first: 'Mary', role: 'admin' },
    },
  }

  assert.equal(
    serialize(document),
    'people:\n  joe:\n    first: Joe\n    last: Schmoe\n  mary:\n    first: Mary\n    role: admin',
  )
})

test('parses inline list arrays', () => {
  assert.deepEqual(parse('tags[3]: admin,ops,dev\n'), { tags: ['admin', 'ops', 'dev'] })
  assert.equal(serialize({ tags: ['admin', 'ops', 'dev'] }), 'tags[3]: admin,ops,dev')
})

test('the root form can be a scalar, an array or an object', () => {
  assert.equal(parse('hello'), 'hello')
  assert.deepEqual(parse('[]'), [])
  assert.deepEqual(parse('[2]: 1,2\n'), [1, 2])
  assert.deepEqual(parse(''), {})
})

test('errors carry the offending line', () => {
  const error = caught(() => parse('person: Ada\n  city: London\n'))

  assert.ok(error instanceof ToonDecodeError)
  assert.equal(error.line, 2)
  assert.equal(error.reason, 'over-indented line')
  assert.match(error.message, /Line 2: over-indented line/)
})

test('decode enforces maxDepth and supports an explicit opt-out', () => {
  assert.deepEqual(parse('a:\n  b:\n    c: 1\n', { maxDepth: 2 }), { a: { b: { c: 1 } } })

  const custom = caught(() => parse('a:\n  b:\n    c: 1\n', { maxDepth: 1 }))
  assert.ok(custom instanceof ToonDecodeError)
  assert.equal(custom.line, 3)
  assert.equal(custom.reason, 'maximum nesting depth exceeded (maxDepth 1)')

  assert.throws(
    () => parse('rows[1]{a{b{c}}}:\n  1\n', { maxDepth: 2 }),
    /maximum nesting depth exceeded \(maxDepth 2\)/,
  )

  const hostile = caught(() => parse(deeplyNestedToon(1001)))
  assert.ok(hostile instanceof ToonDecodeError)
  assert.equal(hostile.line, 1002)
  assert.match(hostile.message, /maxDepth 1000/)

  assert.doesNotThrow(() => parse(deeplyNestedToon(1001), { maxDepth: 0 }))
})

test('serialize enforces maxDepth and supports an explicit opt-out', () => {
  assert.throws(
    () => serialize(deeplyNestedObject(1001)),
    (error) =>
      error instanceof ToonError &&
      error.line === 0 &&
      error.reason === 'maximum nesting depth exceeded (maxDepth 1000)',
  )

  assert.doesNotThrow(() => serialize(deeplyNestedObject(1001), { maxDepth: 0 }))
})

test('rejects array length mismatches', () => {
  const error = caught(() => parse('tags[2]: admin,ops,dev\n'))

  assert.ok(error instanceof ToonDecodeError)
  assert.equal(error.line, 1)
  assert.equal(error.reason, 'expected 2 inline-form values, but got 3')
})

test('strict mode rejects duplicate keys; non-strict takes the last write', () => {
  assert.throws(() => parse('a: 1\na: 2\n'), /duplicate object key/)
  assert.deepEqual(parse('a: 1\na: 2\n', { strict: false }), { a: 2 })
})

test('a dotted key is one literal key', () => {
  // The removed pre-v4 parser split these into nested objects behind an
  // `expandPaths` switch. The canonical decoder has no such mode.
  assert.deepEqual(parse('a.b: 1\n'), { 'a.b': 1 })
  assert.deepEqual(parse('"a.b": 1\n'), { 'a.b': 1 })
  assert.equal(serialize({ 'a.b': 1 }), 'a.b: 1')
})

test('the indent option changes what counts as a level', () => {
  assert.deepEqual(parse('person:\n    city: London\n', { indent: 4 }), {
    person: { city: 'London' },
  })
  assert.throws(() => parse('person:\n    city: London\n', { indent: 3 }), /invalid indentation/)
})

test('serialize quotes exactly what the spec requires', () => {
  assert.equal(
    serialize({ empty: '', numeric: '05', literal: 'true', spaced: ' pad ', dashed: '-x' }),
    'empty: ""\nnumeric: "05"\nliteral: "true"\nspaced: " pad "\ndashed: "-x"',
  )
  // Quoted-looking scalars survive the round-trip as strings, not numbers.
  assert.deepEqual(parse(serialize({ numeric: '05' })), { numeric: '05' })
})

test('treats leading-plus numeric-looking tokens as strings', () => {
  // The spec is silent on leading-plus tokens (upstream spec PR #52); the
  // reference implementation keeps them as strings while exponent plus signs
  // stay numeric.
  assert.deepEqual(parse('values[3]: +1,+1.5,+1e2\nexponent: 1e+2\n'), {
    values: ['+1', '+1.5', '+1e2'],
    exponent: 100,
  })
})

test('nested empty-object list items round-trip as a bare hyphen', () => {
  // The bare `-` marker for an empty object list item applies recursively
  // inside nested expanded arrays, with no trailing space (upstream spec
  // PR #53).
  const input = 'items[2]:\n  - [1]:\n    -\n  - [2]:\n    - x\n    -'

  assert.deepEqual(parse(input), { items: [[{}], ['x', {}]] })
  assert.equal(serialize({ items: [[{}], ['x', {}]] }), input)
})

test('a __proto__ key becomes an own property, not a prototype write', () => {
  const document = parse('"__proto__": 1\n')

  assert.ok(Object.prototype.hasOwnProperty.call(document, '__proto__'))
  assert.equal(document.__proto__, 1)
  assert.equal(({}).polluted, undefined)
})
