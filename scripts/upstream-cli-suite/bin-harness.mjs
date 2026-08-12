import { execFile } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import { afterEach } from 'vitest'

const root = fileURLToPath(new URL('../..', import.meta.url))
const bins = {
  rust: { command: path.join(root, 'target/debug/toon'), prefix: [] },
  ts: {
    command: process.execPath,
    prefix: [path.join(root, 'packages/toon/bin/toon.mjs')],
  },
}
const target = process.env.TOON_CLI_TARGET ?? 'ts'

if (!(target in bins)) throw new Error(`Unknown TOON_CLI_TARGET: ${target}`)

let stdin

export function useTemporaryDirectories() {
  const directories = []

  afterEach(() => {
    while (directories.length > 0) {
      rmSync(directories.pop(), { recursive: true, force: true })
    }
  })

  return (files = {}) => {
    const directory = mkdtempSync(path.join(tmpdir(), 'toon-upstream-cli-'))
    directories.push(directory)

    for (const [relativePath, contents] of Object.entries(files)) {
      const filePath = path.join(directory, relativePath)
      mkdirSync(path.dirname(filePath), { recursive: true })
      writeFileSync(filePath, contents, 'utf8')
    }

    return directory
  }
}

export function mockStdin(input) {
  const previous = stdin
  stdin = input
  return () => {
    stdin = previous
  }
}

export async function runCli(argv, options = {}) {
  const result = await execute(argv, { ...options, stdin })
  return { ...result, exitCode: result.exitCode === 0 ? undefined : result.exitCode }
}

export function runCliProcess(argv, options = {}) {
  return execute(argv, options)
}

function execute(argv, options) {
  const bin = bins[target]

  return new Promise((resolve, reject) => {
    const child = execFile(
      bin.command,
      [...bin.prefix, ...argv],
      { cwd: options.cwd, maxBuffer: 64 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error && typeof error.code !== 'number') reject(error)
        else resolve({ stdout, stderr, exitCode: error ? error.code : 0 })
      },
    )

    child.stdin.on('error', (error) => {
      if (error.code !== 'EPIPE') reject(error)
    })
    child.stdin.end(options.stdin ?? '')
  })
}
