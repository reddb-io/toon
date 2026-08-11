import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), 'utf8')

test('upstream package suite remains a separate CI-enforced compatibility target', async () => {
  const [manifestText, workflow, readme, config, ledgerText] = await Promise.all([
    read('package.json'),
    read('.github/workflows/ci.yml'),
    read('packages/toon/README.md'),
    read('scripts/upstream-package-suite/vitest.config.mjs'),
    read('scripts/upstream-package-suite/skip-ledger.json'),
  ])
  const manifest = JSON.parse(manifestText)
  const ledger = JSON.parse(ledgerText)

  assert.equal(
    manifest.scripts['test:upstream-package'],
    'vitest run --config scripts/upstream-package-suite/vitest.config.mjs',
  )
  assert.match(workflow, /run: pnpm test:upstream-package/)
  assert.match(readme, /drop-in compatible.*upstream package unit suite/is)
  assert.match(config, /vendor\/toon\/packages\/toon\/test/)
  assert.match(config, /packages\/toon\/src\/index\.ts/)
  assert.match(ledger.policy, /entries may only be removed/i)
  assert.ok(Array.isArray(ledger.entries))

  for (const entry of ledger.entries) {
    assert.equal(typeof entry.test, 'string')
    assert.ok(entry.test.endsWith('.test.ts'))
    assert.equal(typeof entry.rationale, 'string')
    assert.ok(entry.rationale.trim().length > 0)
  }
})
