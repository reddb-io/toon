import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), 'utf8')

test('upstream CLI suite remains a CI-enforced bin compatibility target', async () => {
  const [manifestText, workflow, readme, config, harness, ledgerText] = await Promise.all([
    read('package.json'),
    read('.github/workflows/ci.yml'),
    read('packages/toon/README.md'),
    read('scripts/upstream-cli-suite/vitest.config.mjs'),
    read('scripts/upstream-cli-suite/bin-harness.mjs'),
    read('scripts/upstream-cli-suite/skip-ledger.json'),
  ])
  const manifest = JSON.parse(manifestText)
  const ledger = JSON.parse(ledgerText)

  assert.match(manifest.scripts['test:upstream-cli'], /vitest run --config scripts\/upstream-cli-suite\/vitest\.config\.mjs/)
  assert.match(workflow, /run: pnpm test:upstream-cli/)
  assert.match(readme, /drop-in compatible.*upstream CLI suite/is)
  assert.match(config, /vendor\/toon\/packages\/cli\/test/)
  assert.match(config, /packages\/toon\/src\/index\.ts/)
  assert.match(harness, /packages\/toon\/bin\/toon\.mjs/)
  assert.match(harness, /target\/debug\/toon/)
  assert.match(ledger.policy, /entries may only be removed/i)
  assert.ok(Array.isArray(ledger.entries))

  for (const entry of ledger.entries) {
    assert.equal(typeof entry.test, 'string')
    assert.ok(entry.test.endsWith('.test.ts'))
    assert.equal(typeof entry.rationale, 'string')
    assert.ok(entry.rationale.trim().length > 0)
  }
})
