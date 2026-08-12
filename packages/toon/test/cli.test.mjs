/**
 * Golden coverage for the `toon` bin: every flag, both auto-detected modes,
 * stdin and file inputs, `--output` paths, and the exit code each run reports.
 * The expectations are the upstream `@toon-format/cli` contract — this package
 * ships a drop-in front-end for it over the canonical codec.
 */

import assert from 'node:assert/strict'
import test, { after } from 'node:test'

import { ToonDecodeError, encode } from '../dist/index.js'
import { VERSION } from '../dist/version.js'
import { formatDecodeError } from '../dist/cli/format-error.js'
import { jsonStreamFromEvents } from '../dist/cli/json-from-events.js'
import { estimateTokenCount } from '../dist/cli/tokens.js'
import {
  createDirectory,
  readOutput,
  removeDirectories,
  runCliInProcess,
} from './support/cli.mjs'

after(removeDirectories)

const SAMPLE = {
  items: [
    { id: 1, value: 'test' },
    { id: 2, value: 'data' },
  ],
}

test('encode reads stdin and writes TOON to stdout by default', async () => {
  const data = { title: 'TOON test', count: 3, nested: { ok: true } }

  const run = await runCliInProcess([], { stdin: JSON.stringify(data) })

  assert.equal(run.stdout, `${encode(data)}\n`)
  assert.equal(run.stderr, '')
  assert.equal(run.exitCode, 0)
})

test('a bare `-` positional is the same stdin as no positional at all', async () => {
  const data = { ok: true }

  const run = await runCliInProcess(['-'], { stdin: JSON.stringify(data) })

  assert.equal(run.stdout, `${encode(data)}\n`)
  assert.equal(run.exitCode, 0)
})

test('a `.json` input auto-detects encode mode', async () => {
  const cwd = createDirectory({ 'input.json': JSON.stringify(SAMPLE) })

  const run = await runCliInProcess(['input.json'], { cwd })

  assert.equal(run.stdout, 'items[2]{id,value}:\n  1,test\n  2,data\n')
  assert.equal(run.stderr, '')
  assert.equal(run.exitCode, 0)
})

test('a `.toon` input auto-detects decode mode', async () => {
  const cwd = createDirectory({ 'input.toon': encode(SAMPLE) })

  const run = await runCliInProcess(['input.toon'], { cwd })

  assert.deepEqual(JSON.parse(run.stdout), SAMPLE)
  assert.equal(run.exitCode, 0)
})

test('an unknown extension falls back to encode, and `-d` overrides it', async () => {
  const cwd = createDirectory({ 'data.txt': encode(SAMPLE) })

  const fallback = await runCliInProcess(['data.txt'], { cwd })
  assert.equal(fallback.exitCode, 1)
  assert.match(fallback.stderr, /Failed to parse JSON/)

  const forced = await runCliInProcess(['data.txt', '-d'], { cwd })
  assert.deepEqual(JSON.parse(forced.stdout), SAMPLE)
  assert.equal(forced.exitCode, 0)
})

test('`-e` forces encode even for a `.toon` name', async () => {
  const cwd = createDirectory({ 'payload.toon': JSON.stringify({ ok: true }) })

  const run = await runCliInProcess(['payload.toon', '-e'], { cwd })

  assert.equal(run.stdout, 'ok: true\n')
  assert.equal(run.exitCode, 0)
})

test('`-e` wins over `-d` when both are given, including as one cluster', async () => {
  const run = await runCliInProcess(['-ed'], { stdin: '{"a":1}' })

  assert.equal(run.stdout, 'a: 1\n')
  assert.equal(run.exitCode, 0)
})

test('`--output` writes the encoded file and reports the pair on stderr', async () => {
  const cwd = createDirectory({ 'input.json': JSON.stringify(SAMPLE, undefined, 2) })

  const run = await runCliInProcess(['input.json', '--output', 'output.toon'], { cwd })

  assert.equal(readOutput(cwd, 'output.toon'), `${encode(SAMPLE)}\n`)
  assert.equal(run.stdout, '')
  assert.equal(run.stderr, '✔ Encoded `input.json` → `output.toon`\n')
  assert.equal(run.exitCode, 0)
})

test('`-o` accepts its value attached, and labels stdin as the input', async () => {
  const cwd = createDirectory()

  const run = await runCliInProcess(['-ooutput.toon'], { cwd, stdin: '{"key":"value"}' })

  assert.equal(readOutput(cwd, 'output.toon'), 'key: value\n')
  assert.equal(run.stderr, '✔ Encoded `stdin` → `output.toon`\n')
  assert.equal(run.exitCode, 0)
})

test('`--output` writes decoded JSON to a path', async () => {
  const cwd = createDirectory({ 'input.toon': encode(SAMPLE) })

  const run = await runCliInProcess(['input.toon', '-o', 'output.json'], { cwd })

  assert.deepEqual(JSON.parse(readOutput(cwd, 'output.json')), SAMPLE)
  assert.equal(run.stderr, '✔ Decoded `input.toon` → `output.json`\n')
  assert.equal(run.exitCode, 0)
})

test('`--decode` reads TOON from stdin', async () => {
  const data = { items: ['a', 'b'], count: 2 }

  const run = await runCliInProcess(['--decode'], { stdin: encode(data) })

  assert.deepEqual(JSON.parse(run.stdout), data)
  assert.equal(run.exitCode, 0)
})

test('decode round-trips root primitives', async () => {
  for (const [input, expected] of [['42', 42], ['"Hello World"', 'Hello World'], ['true', true]]) {
    const run = await runCliInProcess(['--decode'], { stdin: input })

    assert.equal(JSON.parse(run.stdout), expected)
    assert.equal(run.exitCode, 0)
  }
})

test('`--delimiter` accepts the upstream literals and the readable names', async () => {
  const data = { items: [1, 2, 3] }

  for (const [flag, delimiter] of [
    [',', ','],
    ['comma', ','],
    ['|', '|'],
    ['pipe', '|'],
    ['\t', '\t'],
    ['\\t', '\t'],
    ['tab', '\t'],
  ]) {
    const run = await runCliInProcess(['--delimiter', flag], { stdin: JSON.stringify(data) })

    assert.equal(run.stdout, `${encode(data, { delimiter })}\n`)
    assert.equal(run.exitCode, 0)
  }
})

test('`--indent` follows upstream decimal parseInt semantics', async () => {
  const data = { nested: { deep: { value: 1 } } }

  for (const [flag, indentSize] of [['4', 4], ['0', 0], ['007', 7], ['2.9', 2], ['4abc', 4]]) {
    const run = await runCliInProcess([`--indent=${flag}`], { stdin: JSON.stringify(data) })

    assert.equal(run.stdout, `${encode(data, { indentSize })}\n`)
    assert.equal(run.exitCode, 0)
  }
})

test('an empty `--indent` value falls back to the default', async () => {
  const data = { nested: { deep: 1 } }

  const run = await runCliInProcess(['--indent'], { stdin: JSON.stringify(data) })

  assert.equal(run.stdout, `${encode(data)}\n`)
  assert.equal(run.exitCode, 0)
})

test('`--indent` sets the indentation of decoded JSON', async () => {
  const data = { a: 1, b: [2, 3], c: { nested: true } }
  const cwd = createDirectory({ 'input.toon': encode(data, { indentSize: 4 }) })

  const run = await runCliInProcess(['input.toon', '--decode', '--indent', '4'], { cwd })

  assert.deepEqual(JSON.parse(run.stdout), data)
  assert.match(run.stdout, /\n {4}"a": 1/)
  assert.equal(run.exitCode, 0)
})

test('a zero `--indent` rejects non-empty TOON input, as the decoder does', async () => {
  const run = await runCliInProcess(['--decode', '--indent', '0'], { stdin: 'a: 1\n' })

  assert.equal(run.exitCode, 1)
  assert.match(run.stderr, /invalid indentation/)
})

test('`--no-strict` admits tab indentation that strict decoding rejects', async () => {
  const lenient = await runCliInProcess(['--decode', '--no-strict'], { stdin: 'a:\n\tb: 1\n' })

  assert.deepEqual(JSON.parse(lenient.stdout), { a: { b: 1 } })
  assert.equal(lenient.exitCode, 0)

  const strict = await runCliInProcess(['--decode', '--strict'], { stdin: 'a:\n\tb: 1\n' })

  assert.equal(strict.exitCode, 1)
})

test('`--no-strict` also replaces ill-formed UTF-8 instead of refusing it', async () => {
  const illFormed = new Uint8Array([0x61, 0x3A, 0x20, 0xFF, 0x0A])

  const strict = await runCliInProcess(['--decode'], { stdin: illFormed })
  assert.equal(strict.exitCode, 1)
  assert.match(strict.stderr, /Input is not valid UTF-8\. Pass --no-strict/)

  const lenient = await runCliInProcess(['--decode', '--no-strict'], { stdin: illFormed })
  assert.deepEqual(JSON.parse(lenient.stdout), { a: '\uFFFD' })
  assert.equal(lenient.exitCode, 0)
})

test('`--stats` keeps its estimates on stderr and the payload on stdout', async () => {
  const cwd = createDirectory({ 'input.json': JSON.stringify(SAMPLE) })

  const run = await runCliInProcess(['input.json', '--stats'], { cwd })

  assert.equal(run.stdout, 'items[2]{id,value}:\n  1,test\n  2,data\n')
  assert.equal(
    run.stderr,
    '● Token estimates: ~25 (JSON) → ~14 (TOON)\n✔ Saved ~11 tokens (-44.0%)\n',
  )
  assert.equal(run.exitCode, 0)
})

test('`--stats` with `--output` reports the pair before the estimates', async () => {
  const cwd = createDirectory({ 'input.json': JSON.stringify(SAMPLE) })

  const run = await runCliInProcess(['input.json', '--stats', '-o', 'out.toon'], { cwd })

  assert.equal(readOutput(cwd, 'out.toon'), `${encode(SAMPLE)}\n`)
  assert.equal(run.stdout, '')
  assert.equal(
    run.stderr,
    '✔ Encoded `input.json` → `out.toon`\n'
    + '● Token estimates: ~25 (JSON) → ~14 (TOON)\n'
    + '✔ Saved ~11 tokens (-44.0%)\n',
  )
  assert.equal(run.exitCode, 0)
})

test('a large document streams to the same bytes as a one-shot encode', async () => {
  const data = {
    items: Array.from({ length: 1000 }, (_, index) => ({
      id: index,
      name: `Item ${index}`,
      value: index / 7,
    })),
  }
  const cwd = createDirectory({ 'large.json': JSON.stringify(data) })

  const encoded = await runCliInProcess(['large.json', '-o', 'large.toon'], { cwd })
  assert.equal(readOutput(cwd, 'large.toon'), `${encode(data)}\n`)
  assert.equal(encoded.exitCode, 0)

  const decoded = await runCliInProcess(['large.toon', '-o', 'large.out.json'], { cwd })
  assert.deepEqual(JSON.parse(readOutput(cwd, 'large.out.json')), data)
  assert.equal(decoded.exitCode, 0)
})

test('invalid JSON fails with the upstream message and exit code 1', async () => {
  const run = await runCliInProcess([], { stdin: '{ invalid json }' })

  assert.equal(run.stdout, '')
  assert.match(run.stderr, /^✖ Failed to parse JSON: /)
  assert.equal(run.exitCode, 1)
})

test('a decode failure renders line context, source, and a caret', async () => {
  const run = await runCliInProcess(['--decode'], { stdin: 'a:\n\tb: 1\n' })

  assert.equal(
    run.stderr,
    '✖ Failed to decode TOON at line 2: tab used as indentation\n\n  2 | →b: 1\n      ^\n',
  )
  assert.doesNotMatch(run.stderr, /^\s+at \S+/m)
  assert.equal(run.exitCode, 1)
})

test('`--verbose` appends the cause chain and the stack trace', async () => {
  const run = await runCliInProcess(['--decode', '--verbose'], { stdin: 'a:\n\tb: 1\n' })

  assert.match(run.stderr, /^✖ Failed to decode TOON at line 2: tab used as indentation/)
  assert.match(run.stderr, /Caused by: ToonError: line 2: tab used as indentation/)
  assert.match(run.stderr, /^\s+at \S+/m)
  assert.equal(run.exitCode, 1)
})

test('an invalid delimiter is rejected before any conversion runs', async () => {
  const cwd = createDirectory({ 'input.json': JSON.stringify({ value: 1 }) })

  const run = await runCliInProcess(['input.json', '--delimiter', ';'], { cwd })

  assert.equal(
    run.stderr,
    '✖ Invalid delimiter ";". Valid delimiters are: comma (,), tab (\\t), pipe (|)\n',
  )
  assert.equal(run.exitCode, 1)
})

test('a non-numeric or negative `--indent` is rejected with the raw value', async () => {
  for (const value of ['abc', '-1', '-42']) {
    const run = await runCliInProcess(['--indent', value], { stdin: '{}' })

    assert.equal(run.stderr, `✖ Invalid indent value: ${value}\n`)
    assert.equal(run.exitCode, 1)
  }
})

test('a missing input file reports the path without a stack trace', async () => {
  const cwd = createDirectory()

  const run = await runCliInProcess(['nonexistent.json'], { cwd })

  assert.match(run.stderr, /nonexistent\.json/)
  assert.doesNotMatch(run.stderr, /^\s+at \S+/m)
  assert.equal(run.exitCode, 1)
})

test('an unknown flag or a surplus positional names itself in the failure', async () => {
  const unknownFlag = await runCliInProcess(['--jsonn'], { stdin: '{}' })
  assert.equal(unknownFlag.stderr, '✖ Unknown argument(s): --jsonn – see --help\n')
  assert.equal(unknownFlag.exitCode, 1)

  const unknownShort = await runCliInProcess(['-z'], { stdin: '{}' })
  assert.equal(unknownShort.stderr, '✖ Unknown argument(s): -z – see --help\n')

  const surplus = await runCliInProcess(['one.json', 'two.json'])
  assert.equal(surplus.stderr, '✖ Unknown argument(s): "two.json" – see --help\n')
  assert.equal(surplus.exitCode, 1)
})

test('`--` ends option parsing, so a dashed file name stays a positional', async () => {
  const cwd = createDirectory({ '--odd.json': JSON.stringify({ ok: true }) })

  const run = await runCliInProcess(['--', '--odd.json'], { cwd })

  assert.equal(run.stdout, 'ok: true\n')
  assert.equal(run.exitCode, 0)
})

test('`--help` and `--version` answer on stdout and exit successfully', async () => {
  for (const flag of ['--help', '-h']) {
    const run = await runCliInProcess([flag])

    assert.match(run.stdout, /^TOON CLI – Convert between JSON and TOON\n/)
    assert.match(run.stdout, /-o, --output <file>/)
    assert.match(run.stdout, /--no-strict/)
    assert.equal(run.exitCode, 0)
  }

  for (const flag of ['--version', '-v']) {
    const run = await runCliInProcess([flag])

    assert.equal(run.stdout, `${VERSION}\n`)
    assert.equal(run.exitCode, 0)
  }
})

test('boolean flags accept an explicit value, and text stdin decodes too', async () => {
  const off = await runCliInProcess(['--decode', '--strict=false'], { stdin: 'a:\n\tb: 1\n' })
  assert.deepEqual(JSON.parse(off.stdout), { a: { b: 1 } })
  assert.equal(off.exitCode, 0)

  const zero = await runCliInProcess(['--decode', '--strict=0'], { stdin: 'a:\n\tb: 1\n' })
  assert.equal(zero.exitCode, 0)

  const on = await runCliInProcess(['--stats=true'], { stdin: '{"a":1}' })
  assert.match(on.stderr, /Token estimates/)
})

test('the JSON writer emits the compact form at indent 0', async () => {
  const events = [
    { type: 'startObject', line: 1 },
    { type: 'key', key: 'items', line: 1 },
    { type: 'startArray', length: 2, line: 1 },
    { type: 'primitive', value: 1, line: 2 },
    { type: 'startObject', line: 3 },
    { type: 'key', key: 'ok', line: 3 },
    { type: 'primitive', value: true, line: 3 },
    { type: 'endObject', line: 3 },
    { type: 'endArray', line: 3 },
    { type: 'endObject', line: 3 },
  ]

  assert.equal(await collect(events, 0), '{"items":[1,{"ok":true}]}')
  assert.equal(await collect(events, 2), JSON.stringify({ items: [1, { ok: true }] }, undefined, 2))
})

test('the JSON writer refuses a malformed event stream', async () => {
  const cases = [
    [[{ type: 'startObject', line: 1 }, { type: 'endArray', line: 1 }], /Mismatched endArray/],
    [[{ type: 'startArray', length: 0, line: 1 }, { type: 'endObject', line: 1 }], /Mismatched endObject/],
    [[{ type: 'key', key: 'a', line: 1 }], /Key event outside of object context/],
    [
      [{ type: 'startObject', line: 1 }, { type: 'primitive', value: 1, line: 1 }],
      /Primitive event without preceding key/,
    ],
    [[{ type: 'startObject', line: 1 }], /Incomplete event stream/],
  ]

  for (const [events, expected] of cases) {
    await assert.rejects(() => collect(events, 2), expected)
  }
})

test('a decode error without source context renders the header alone', () => {
  const error = new ToonDecodeError('unexpected end of input', { line: 7 })

  assert.equal(
    formatDecodeError(error),
    'Failed to decode TOON at line 7: unexpected end of input',
  )
})

test('the estimator reproduces the tokenx 1.3.0 numbers the Rust front-end reports', () => {
  const cases = [
    ['', 0],
    ['   \n\t ', 0],
    ['{}', 1],
    ['hello', 1],
    ['1234567890', 1],
    ['Supercalifragilisticexpialidocious', 6],
    ['Größenänderung Straße', 7],
    ['café à la crème œuvre', 8],
    ['zażółć gęślą jaźń', 6],
    ['東京の日本語テキスト', 10],
    ['한국어', 3],
    ['snake_case-and.dots/slashes', 10],
  ]

  for (const [text, expected] of cases) {
    assert.equal(estimateTokenCount(text), expected, `estimate for ${JSON.stringify(text)}`)
  }
})

async function collect(events, indent) {
  const pieces = []
  for await (const piece of jsonStreamFromEvents(toAsync(events), indent)) pieces.push(piece)
  return pieces.join('')
}

async function* toAsync(events) {
  for (const event of events) yield event
}
