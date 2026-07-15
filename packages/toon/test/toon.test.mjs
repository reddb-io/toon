import assert from 'node:assert/strict'
import test from 'node:test'

import { ToonError, parse, parseDocument, serialize } from '../src/index.js'

/** `assert.throws` returns nothing, so capture the error to inspect its line. */
function caught(fn) {
  try {
    fn()
  } catch (error) {
    return error
  }
  return assert.fail('expected a throw')
}

test('parses flat fields and serializes canonical TOON', () => {
  const document = parse('name : Ada\nactive: true\ncount: 3\n')

  assert.deepEqual(document, { name: 'Ada', active: true, count: 3 })
  assert.equal(serialize(document), 'name: Ada\nactive: true\ncount: 3\n')
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
    'users[2]{id,name,active}:\n  1,Ada,true\n  2,Bob Smith,false\n',
  )
})

test('parses inline list arrays', () => {
  assert.deepEqual(parse('tags[3]: admin,ops,dev\n'), { tags: ['admin', 'ops', 'dev'] })
  assert.equal(serialize({ tags: ['admin', 'ops', 'dev'] }), 'tags[3]: admin,ops,dev\n')
})

test('the root form can be a scalar, an array or an object', () => {
  assert.equal(parse('hello'), 'hello')
  assert.deepEqual(parse('[]'), [])
  assert.deepEqual(parse('[2]: 1,2\n'), [1, 2])
  assert.deepEqual(parse(''), {})
})

test('errors carry the offending line', () => {
  const error = caught(() => parse('person: Ada\n  city: London\n'))

  assert.ok(error instanceof ToonError)
  assert.equal(error.line, 2)
  assert.equal(error.reason, 'invalid indentation')
  assert.match(error.message, /line 2: invalid indentation/)
})

test('rejects array length mismatches', () => {
  const error = caught(() => parse('tags[2]: admin,ops,dev\n'))

  assert.ok(error instanceof ToonError)
  assert.equal(error.line, 1)
  assert.equal(error.reason, 'array length mismatch')
})

test('strict mode rejects duplicate keys; non-strict takes the last write', () => {
  assert.throws(() => parse('a: 1\na: 2\n'), /duplicate key/)
  assert.deepEqual(parse('a: 1\na: 2\n', { strict: false }), { a: 2 })
})

test('expandPaths turns dotted keys into nested objects', () => {
  assert.deepEqual(parse('a.b: 1\n'), { 'a.b': 1 })
  assert.deepEqual(parse('a.b: 1\n', { expandPaths: 'safe' }), { a: { b: 1 } })
  // A quoted key is a literal key, never a path.
  assert.deepEqual(parse('"a.b": 1\n', { expandPaths: 'safe' }), { 'a.b': 1 })
})

test('the indent option changes what counts as a level', () => {
  assert.deepEqual(parse('person:\n    city: London\n', { indent: 4 }), {
    person: { city: 'London' },
  })
  assert.throws(() => parse('person:\n    city: London\n', { indent: 3 }), /invalid indentation/)
})

test('parseDocument insists on an object root', () => {
  assert.deepEqual(parseDocument('name: Ada\n'), { name: 'Ada' })
  assert.throws(() => parseDocument('hello'), /expected `key: value`/)
})

test('serialize quotes exactly what the spec requires', () => {
  assert.equal(
    serialize({ empty: '', numeric: '05', literal: 'true', spaced: ' pad ', dashed: '-x' }),
    'empty: ""\nnumeric: "05"\nliteral: "true"\nspaced: " pad "\ndashed: "-x"\n',
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
  const input = 'items[2]:\n  - [1]:\n    -\n  - [2]:\n    - x\n    -\n'

  assert.deepEqual(parse(input), { items: [[{}], ['x', {}]] })
  assert.equal(serialize({ items: [[{}], ['x', {}]] }), input)
})

test('a __proto__ key becomes an own property, not a prototype write', () => {
  const document = parse('"__proto__": 1\n')

  assert.ok(Object.prototype.hasOwnProperty.call(document, '__proto__'))
  assert.equal(document.__proto__, 1)
  assert.equal(({}).polluted, undefined)
})
