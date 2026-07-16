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
const READMES = [
  join(REPO_ROOT, 'README.md'),
  join(REPO_ROOT, 'packages/toon/README.md'),
]

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
  const directory = mkdtempSync(join(tmpdir(), 'toon-readme-'))
  let counter = 0
  try {
    for (const readme of READMES) {
      const examples = readmeExamples(readFileSync(readme, 'utf8'))
      assert.ok(examples.length >= 2, `expected JS examples in ${readme}, found ${examples.length}`)

      examples.forEach((example, index) => {
        const label = `${readme} example ${index + 1}`
        assert.match(example.source, /@reddb-io\/toon/, `${label} imports the published package`)
        assert.equal(run(example.source, directory, (counter += 1)), example.expected, label)
      })
    }
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
