/**
 * Harness for the `toon` bin: an in-process run over the CLI's io seam, and a
 * child-process run through the published `bin/toon.mjs`, where the exit code
 * the shell sees is real.
 */

import { execFile } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import process from 'node:process'
import { Readable } from 'node:stream'
import { fileURLToPath } from 'node:url'

import { runCli } from '../../dist/cli/run.js'

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')

export const BIN_PATH = path.join(packageRoot, 'bin/toon.mjs')

const directories = []

/** Creates a throwaway directory seeded with `files`, removed by `removeDirectories`. */
export function createDirectory(files = {}) {
  const directory = mkdtempSync(path.join(tmpdir(), 'toon-cli-test-'))
  directories.push(directory)

  for (const [relativePath, contents] of Object.entries(files)) {
    const filePath = path.join(directory, relativePath)
    mkdirSync(path.dirname(filePath), { recursive: true })
    writeFileSync(filePath, contents)
  }

  return directory
}

export function removeDirectories() {
  while (directories.length > 0) {
    rmSync(directories.pop(), { recursive: true, force: true })
  }
}

export function readOutput(directory, relativePath) {
  return readFileSync(path.join(directory, relativePath), 'utf8')
}

/** Runs one invocation in-process, capturing both streams and the exit code. */
export async function runCliInProcess(argv, options = {}) {
  const stdout = []
  const stderr = []

  const exitCode = await runCli(argv, {
    cwd: options.cwd ?? process.cwd(),
    stdout: (text) => stdout.push(text),
    stderr: (text) => stderr.push(text),
    stdin: () => Readable.from(stdinChunks(options.stdin)),
  })

  return { stdout: stdout.join(''), stderr: stderr.join(''), exitCode }
}

/** Runs the shipped bin as a child process, so exit status and flushing are real. */
export function runCliProcess(argv, options = {}) {
  return new Promise((resolve, reject) => {
    const child = execFile(
      process.execPath,
      [BIN_PATH, ...argv],
      { cwd: options.cwd, maxBuffer: 64 * 1024 * 1024 },
      (caught, stdout, stderr) => {
        // A numeric `code` is the child's exit status; anything else failed to spawn.
        if (caught && typeof caught.code !== 'number') reject(caught)
        else resolve({ stdout, stderr, exitCode: caught ? caught.code : 0 })
      },
    )

    child.stdin.end(options.stdin ?? '')
  })
}

function stdinChunks(input) {
  if (input === undefined) return []
  if (typeof input === 'string') return [new TextEncoder().encode(input)]
  return [input]
}
