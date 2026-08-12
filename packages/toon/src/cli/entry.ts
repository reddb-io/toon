/**
 * Binds the `toon` CLI to the real process. `bin/toon.mjs` imports this module.
 *
 * The exit code is set rather than forced: `process.exit` would discard whatever
 * stdout still has buffered, truncating a piped result partway through.
 */

import process from 'node:process'

import { runCli } from './run.js'

const exitCode = await runCli(process.argv.slice(2), {
  cwd: process.cwd(),
  stdout: (text) => { process.stdout.write(text) },
  stderr: (text) => { process.stderr.write(text) },
  stdin: () => process.stdin,
})

if (exitCode !== 0) {
  process.exitCode = exitCode
}
