/**
 * The `toon` bin against the shared cross-language CLI corpus.
 *
 * `tests/golden/toon-cli/` pins one invocation per directory: the argv, the
 * stdin, the files in the working directory, and the exact stdout, stderr,
 * exit code, and written files it must produce. `tests/runners/rust/toon/
 * cli_golden.rs` drives the same corpus through the Rust bin, so a case that
 * passes on both sides is byte parity between the two front-ends — the
 * contract Spec #359 asks for.
 */

import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import path from 'node:path'
import test, { after } from 'node:test'
import { fileURLToPath } from 'node:url'

import { createDirectory, removeDirectories, runCliProcess } from './support/cli.mjs'

after(removeDirectories)

const CORPUS = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../../tests/golden/toon-cli',
)

const cases = readdirSync(CORPUS)
  .filter((name) => statSync(path.join(CORPUS, name)).isDirectory())
  .sort()

test('the shared CLI corpus is not empty', () => {
  assert.ok(cases.length > 0, 'the shared CLI corpus should hold cases')
})

for (const name of cases) {
  test(`golden CLI case: ${name}`, async () => {
    const directory = path.join(CORPUS, name)
    const expected = readCase(directory)
    const cwd = createDirectory(expected.files)

    const run = await runCliProcess(expected.args, { cwd, stdin: expected.stdin })

    assert.equal(run.stdout, expected.stdout, `${name} stdout`)
    assert.equal(run.stderr, expected.stderr, `${name} stderr`)
    assert.equal(run.exitCode, expected.exitCode, `${name} exit code`)

    for (const [file, contents] of Object.entries(expected.outputs)) {
      assert.equal(readFileSync(path.join(cwd, file), 'utf8'), contents, `${name} wrote ${file}`)
    }
  })
}

function readCase(directory) {
  return {
    args: readArgs(path.join(directory, 'args.txt')),
    stdin: read(path.join(directory, 'stdin.txt')) ?? '',
    files: readDirectory(path.join(directory, 'files')),
    stdout: required(directory, 'stdout.txt'),
    stderr: required(directory, 'stderr.txt'),
    exitCode: Number.parseInt(required(directory, 'exit.txt').trim(), 10),
    outputs: readDirectory(path.join(directory, 'output')),
  }
}

/**
 * One argument per line, so an argument may hold spaces. The trailing newline
 * the file ends with is a terminator, not an empty final argument.
 */
function readArgs(file) {
  const text = read(file)
  assert.ok(text !== undefined, `${file} is required`)
  return text.replace(/\n$/, '').split('\n').filter((line) => line !== '')
}

function required(directory, name) {
  const text = read(path.join(directory, name))
  assert.ok(text !== undefined, `${path.basename(directory)} needs ${name}`)
  return text
}

function readDirectory(directory) {
  let names
  try {
    names = readdirSync(directory)
  } catch {
    return {}
  }
  return Object.fromEntries(
    names.sort().map((name) => [name, readFileSync(path.join(directory, name), 'utf8')]),
  )
}

function read(file) {
  try {
    return readFileSync(file, 'utf8')
  } catch {
    return undefined
  }
}
