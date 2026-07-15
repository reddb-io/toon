/**
 * The parity contract: the JS implementation runs the *same* corpus as the Rust
 * crate, under the *same* criteria as `tests/toon/spec_conformance.rs`, and must
 * pass all of it. There is no expected-failure ledger here — a JS-only exception
 * would be exactly the drift this test exists to prevent.
 *
 * Official TOON fixtures come from the `vendor/toon-spec` submodule; the TOONL
 * ones from `tests/toonl/fixtures`, shared with the Rust harness.
 */

import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'
import test from 'node:test'

import {
  ToonlEncoder,
  closeTransform,
  closeTransformInterleaved,
  encodeLines,
  encodeRecords,
  parse,
  parseStream,
  serialize,
} from '../src/index.js'

const REPO_ROOT = fileURLToPath(new URL('../../../', import.meta.url))
const FIXTURE_ROOT = join(REPO_ROOT, 'vendor/toon-spec/tests/fixtures')
const LOCAL_FIXTURE_ROOT = join(REPO_ROOT, 'tests/toon/fixtures')
const TOONL_FIXTURE_ROOT = join(REPO_ROOT, 'tests/toonl/fixtures')

function readFixtures(directory) {
  let entries
  try {
    entries = readdirSync(directory)
  } catch {
    assert.fail(`spec fixtures missing at ${directory} — run \`git submodule update --init\``)
  }
  return entries
    .filter((entry) => entry.endsWith('.json'))
    .sort()
    .map((entry) => ({
      file: entry,
      fixture: JSON.parse(readFileSync(join(directory, entry), 'utf8')),
    }))
}

/**
 * JSON-value equality: key order is irrelevant, and numbers compare with `===`
 * so `-0` and `0` are the same JSON number (they are, once serialized).
 */
function jsonEqual(left, right) {
  if (Array.isArray(left) || Array.isArray(right)) {
    return (
      Array.isArray(left) &&
      Array.isArray(right) &&
      left.length === right.length &&
      left.every((value, index) => jsonEqual(value, right[index]))
    )
  }
  if (left !== null && right !== null && typeof left === 'object' && typeof right === 'object') {
    const leftKeys = Object.keys(left)
    const rightKeys = Object.keys(right)
    return (
      leftKeys.length === rightKeys.length &&
      leftKeys.every(
        (key) =>
          Object.prototype.hasOwnProperty.call(right, key) && jsonEqual(left[key], right[key]),
      )
    )
  }
  return left === right
}

/**
 * Decoder options a fixture is written against. Encoder-only options
 * (`delimiter`, `keyFolding`, `flattenDepth`) carry no decoder meaning and are
 * ignored; `indent` is shared by both sides.
 */
function decoderOptions(options) {
  if (options === undefined || options === null) {
    return {}
  }
  return {
    indent: options.indent,
    strict: options.strict,
    expandPaths: options.expandPaths === 'safe',
  }
}

function encoderOptions(options) {
  if (options === undefined || options === null) {
    return {}
  }
  return {
    nestedTabularHeaders: options.nestedTabularHeaders === true,
    keyedMapCollapse: options.keyedMapCollapse === true,
  }
}

/** Our canonical output has to decode back to the value we started from. */
function roundTripsTo(value) {
  // The canonical profile is always the default one, whatever the input used.
  return jsonEqual(parse(serialize(value)), value)
}

test('official TOON spec fixtures all pass', () => {
  const failures = []
  let executed = 0

  for (const category of ['decode', 'encode']) {
    const fixtures = [
      ...readFixtures(join(FIXTURE_ROOT, category)),
      ...readFixtures(join(LOCAL_FIXTURE_ROOT, category)),
    ]
    for (const { file, fixture } of fixtures) {
      for (const testCase of fixture.tests) {
        const id = `${category}/${file}::${testCase.name}`
        executed += 1
        const options = decoderOptions(testCase.options)

        let passed = false
        try {
          if (category === 'decode') {
            if (testCase.shouldError === true) {
              // A rejection the spec asked for.
              try {
                parse(testCase.input, options)
                passed = false
              } catch {
                passed = true
              }
            } else {
              // Parsing without an error is not enough. The decoded value has to
              // be the one the spec says it is, and our own canonical output has
              // to decode back to that same value — otherwise either the parser
              // returns wrong data silently, or the serializer emits TOON we
              // cannot read.
              const value = parse(testCase.input, options)
              passed = jsonEqual(value, testCase.expected) && roundTripsTo(value)
              if (passed && testCase.failClosedV3Strict === true) {
                assert.throws(() => rejectV3Strict(testCase.input), /invalid keyed map header/)
              }
            }
          } else {
            if (
              Object.prototype.hasOwnProperty.call(testCase, 'input') &&
              testCase.options?.keyedMapCollapse === true
            ) {
              const encoded = serialize(testCase.input, encoderOptions(testCase.options))
              const decoded = parse(testCase.expected, options)
              passed = encoded === testCase.expected && jsonEqual(decoded, testCase.input)
            } else {
              const value = parse(testCase.expected, options)
              passed = roundTripsTo(value)
            }
          }
        } catch (error) {
          failures.push(`${id} — threw: ${error.message}`)
          continue
        }

        if (!passed) {
          failures.push(id)
        }
      }
    }
  }

  assert.deepEqual(failures, [], `TOON conformance failures:\n  ${failures.join('\n  ')}`)
  // A corpus that silently reads zero cases would pass the assertion above, so
  // the count is part of the contract: this is the whole official corpus or bust.
  assert.ok(executed >= 380, `expected the full spec corpus, ran only ${executed} cases`)
})

test('TOONL fixtures are executable spec examples', () => {
  let executed = 0

  for (const { fixture } of readFixtures(TOONL_FIXTURE_ROOT)) {
    assert.ok(
      ['toonl-v0.1', 'toonl-v0.2'].includes(fixture.version),
      `fixture declares a supported TOONL spec version, got ${JSON.stringify(fixture.version)}`,
    )
    assert.equal(fixture.extension, '.toonl', 'fixture declares the canonical extension')
    assert.equal(fixture.mediaHint, 'application/toonl', 'fixture declares the media hint')

    for (const testCase of fixture.tests) {
      const name = testCase.name
      executed += 1

      if (testCase.kind === 'decode') {
        assert.deepEqual(parseStream(testCase.input), testCase.segments, `${name}: decoded segments`)
        continue
      }

      if (testCase.kind === 'encode') {
        const encoder = new ToonlEncoder(testCase.delimiter ?? ',', testCase.fields)
        if (testCase.continuationEveryRows !== undefined) {
          encoder.setContinuationEveryRows(testCase.continuationEveryRows)
        }
        for (const row of testCase.rows) {
          encoder.pushRawRow(row)
        }
        assert.equal(encoder.finish(), testCase.expected, `${name}: encoded`)
        continue
      }

      if (testCase.kind === 'encode-records') {
        assert.equal(encodeRecords(testCase.records), testCase.expected, `${name}: encoded`)
        continue
      }

      if (testCase.kind === 'encode-tagged-records') {
        const emitter = encodeLines()
        let output = ''
        for (const operation of testCase.operations) {
          output += emitter.pushTagged(operation.tag, operation.record)
        }
        assert.equal(output + emitter.end(), testCase.expected, `${name}: encoded`)
        continue
      }

      if (testCase.kind === 'close-transform') {
        const documents = closeTransform(testCase.input)
        assert.deepEqual(documents, testCase.expectedToonDocuments, `${name}: transformed docs`)
        for (const document of documents) {
          parse(document)
        }
        if (testCase.expectedInterleavedToonDocuments !== undefined) {
          const interleavedDocuments = closeTransformInterleaved(testCase.input)
          assert.deepEqual(
            interleavedDocuments,
            testCase.expectedInterleavedToonDocuments,
            `${name}: interleaved transformed docs`,
          )
          for (const document of interleavedDocuments) {
            parse(document)
          }
        }
        continue
      }

      if (testCase.kind === 'error') {
        assert.throws(
          () => closeTransform(testCase.input),
          (error) => error.message.includes(testCase.expectedError),
          `${name}: expected error containing ${JSON.stringify(testCase.expectedError)}`,
        )
        continue
      }

      if (testCase.kind === 'v0.1-error') {
        assert.throws(
          () => rejectV01Toonl(testCase.input),
          (error) => error.message.includes(testCase.expectedError),
          `${name}: expected v0.1 error containing ${JSON.stringify(testCase.expectedError)}`,
        )
        continue
      }

      assert.fail(`${name}: unknown TOONL fixture kind ${testCase.kind}`)
    }
  }

  assert.ok(executed >= 14, `expected the TOONL corpus, ran only ${executed} cases`)
})

function rejectV01Toonl(input) {
  input.split(/\n/).forEach((rawLine, index) => {
    const lineNumber = index + 1
    const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine
    if (line === '') {
      return
    }
    if (!line.startsWith('[')) {
      if (/^[A-Za-z0-9_-]+:/.test(line)) {
        throw new Error(`line ${lineNumber}: row arity mismatch`)
      }
      return
    }
    const close = line.indexOf(']')
    if (close === -1) {
      throw new Error(`line ${lineNumber}: invalid header`)
    }
    const bracket = line.slice(1, close)
    if (bracket.startsWith('=')) {
      return
    }
    if (!['', '|', '\t'].includes(bracket)) {
      throw new Error(`line ${lineNumber}: invalid header delimiter`)
    }
    const suffix = line.slice(close + 1)
    if (!suffix.startsWith('{') || !suffix.endsWith('}:')) {
      throw new Error(`line ${lineNumber}: invalid header`)
    }
  })
}

function rejectV3Strict(input) {
  input.split(/\n/).forEach((rawLine, index) => {
    const lineNumber = index + 1
    const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine
    if (/^[ ]*[^:[\n]+{.*}:[ ]*$/.test(line)) {
      throw new Error(`line ${lineNumber}: invalid keyed map header`)
    }
  })
}
