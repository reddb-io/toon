/**
 * The `toon` bin as a child process, where the exit status the shell sees, the
 * shebang, and stdout flushing are all real. `npx @reddb-io/toon …` resolves to
 * exactly this entry point.
 */

import assert from 'node:assert/strict'
import { accessSync, constants, readFileSync } from 'node:fs'
import test, { after } from 'node:test'

import { encode } from '../dist/index.js'
import { VERSION } from '../dist/version.js'
import {
  BIN_PATH,
  createDirectory,
  readOutput,
  removeDirectories,
  runCliProcess,
} from './support/cli.mjs'

after(removeDirectories)

test('the bin is an executable node script that loads the built entry point', () => {
  const source = readFileSync(BIN_PATH, 'utf8')

  assert.match(source, /^#!\/usr\/bin\/env node\n/)
  assert.match(source, /dist\/cli\/entry\.js/)
  accessSync(BIN_PATH, constants.X_OK)

  const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
  assert.equal(manifest.bin.toon, 'bin/toon.mjs')
  assert.ok(manifest.files.includes('bin/'), 'the bin directory must be published')
})

test('`toon --version` prints the package version and exits 0', async () => {
  const run = await runCliProcess(['--version'])

  assert.equal(run.stdout, `${VERSION}\n`)
  assert.equal(run.stderr, '')
  assert.equal(run.exitCode, 0)
})

test('`toon input.json` encodes a file and exits 0', async () => {
  const data = { items: ['alpha', 'beta'] }
  const cwd = createDirectory({ 'input.json': JSON.stringify(data) })

  const run = await runCliProcess(['input.json'], { cwd })

  assert.equal(run.stdout, `${encode(data)}\n`)
  assert.equal(run.exitCode, 0)
})

test('`cat data.json | toon` encodes stdin and exits 0', async () => {
  const data = { name: 'Ada' }

  const run = await runCliProcess([], { stdin: JSON.stringify(data) })

  assert.equal(run.stdout, `${encode(data)}\n`)
  assert.equal(run.exitCode, 0)
})

test('`toon data.toon -o out.json` decodes to a path and exits 0', async () => {
  const data = { items: [1, 2, 3], meta: { done: false } }
  const cwd = createDirectory({ 'data.toon': encode(data) })

  const run = await runCliProcess(['data.toon', '-o', 'out.json'], { cwd })

  assert.deepEqual(JSON.parse(readOutput(cwd, 'out.json')), data)
  assert.equal(run.stdout, '')
  assert.match(run.stderr, /Decoded `data\.toon` → `out\.json`/)
  assert.equal(run.exitCode, 0)
})

test('`toon -e -o out.toon in.json` runs the upstream flag order unmodified', async () => {
  const data = { ok: true }
  const cwd = createDirectory({ 'in.json': JSON.stringify(data) })

  const run = await runCliProcess(['-e', '-o', 'out.toon', 'in.json'], { cwd })

  assert.equal(readOutput(cwd, 'out.toon'), `${encode(data)}\n`)
  assert.equal(run.exitCode, 0)
})

test('a missing input exits 1 with an empty stdout', async () => {
  const cwd = createDirectory()

  const run = await runCliProcess(['nonexistent.json'], { cwd })

  assert.equal(run.stdout, '')
  assert.match(run.stderr, /nonexistent\.json/)
  assert.equal(run.exitCode, 1)
})

test('a piped result survives the failure exit code of a later run', async () => {
  const run = await runCliProcess(['--decode'], { stdin: 'a:\n\tb: 1\n' })

  assert.equal(run.stdout, '')
  assert.match(run.stderr, /^✖ Failed to decode TOON at line 2:/)
  assert.equal(run.exitCode, 1)
})
