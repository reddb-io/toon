import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

const root = fileURLToPath(new URL('../..', import.meta.url))
const ledger = JSON.parse(
  await readFile(new URL('./skip-ledger.json', import.meta.url), 'utf8'),
)

for (const entry of ledger.entries) {
  if (
    typeof entry.test !== 'string'
    || !entry.test.endsWith('.test.ts')
    || typeof entry.rationale !== 'string'
    || entry.rationale.trim().length === 0
  ) {
    throw new Error('Every upstream-suite skip needs a test file and rationale')
  }
}

const localEntry = fileURLToPath(
  new URL('../../packages/toon/src/index.ts', import.meta.url),
)
const upstreamTests = 'vendor/toon/packages/toon/test'

export default defineConfig({
  resolve: {
    alias: [
      { find: /^\.\.\/src\/index(?:\.ts)?$/, replacement: localEntry },
      { find: /^\.\.\/src\/decode\/event-builder(?:\.ts)?$/, replacement: localEntry },
    ],
  },
  test: {
    root,
    include: [`${upstreamTests}/**/*.test.ts`],
    exclude: ledger.entries.map(({ test }) => `${upstreamTests}/${test}`),
  },
})
