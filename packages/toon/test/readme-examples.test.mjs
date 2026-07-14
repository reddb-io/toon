/**
 * Runs the JS examples published in README.md and checks their real stdout
 * against the `console` block each one advertises, so the docs cannot drift
 * from the API (the reddb driver contract).
 *
 * Every ```js block immediately followed by a ```console block is executed in a
 * child node process, with the `@reddb-io/toon` import rewritten to this
 * checkout — otherwise the examples would only pass once the package is
 * published, which is exactly backwards.
 */

import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const REPO_ROOT = fileURLToPath(new URL('../../../', import.meta.url))
const ENTRY_POINT = fileURLToPath(new URL('../src/index.js', import.meta.url))
const README = join(REPO_ROOT, 'README.md')

/** Pairs each ```js block with the ```console block that follows it. */
function readmeExamples(markdown) {
  const examples = []
  const block = /```js\n([\s\S]*?)```\n+```console\n([\s\S]*?)```/g

  let match = block.exec(markdown)
  while (match !== null) {
    examples.push({ source: match[1], expected: match[2] })
    match = block.exec(markdown)
  }
  return examples
}

function run(source, directory, index) {
  const file = join(directory, `example-${index}.mjs`)
  writeFileSync(file, source.replaceAll("'@reddb-io/toon'", JSON.stringify(ENTRY_POINT)))
  return execFileSync(process.execPath, [file], { encoding: 'utf8' })
}

test('README JS examples produce the output they advertise', () => {
  const examples = readmeExamples(readFileSync(README, 'utf8'))
  assert.ok(examples.length >= 2, `expected the README's JS examples, found ${examples.length}`)

  const directory = mkdtempSync(join(tmpdir(), 'toon-readme-'))
  try {
    examples.forEach((example, index) => {
      assert.match(
        example.source,
        /@reddb-io\/toon/,
        `example ${index + 1} imports the published package`,
      )
      assert.equal(run(example.source, directory, index), example.expected, `example ${index + 1}`)
    })
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
