import { readFile } from 'node:fs/promises'
import { relative } from 'node:path'
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
    || (entry.case !== undefined && typeof entry.case !== 'string')
  ) {
    throw new Error('Every upstream-suite skip needs a test file and rationale')
  }
}

const localEntry = fileURLToPath(
  new URL('../../packages/toon/src/index.ts', import.meta.url),
)
const upstreamTests = 'vendor/toon/packages/toon/test'
const upstreamTestRoot = fileURLToPath(
  new URL('../../vendor/toon/packages/toon/test/', import.meta.url),
)

function skipLedgerCases() {
  return {
    name: 'upstream-skip-ledger',
    enforce: 'pre',
    transform(code, id) {
      const test = relative(upstreamTestRoot, id.split('?')[0]).replaceAll('\\', '/')
      const skips = ledger.entries.filter((entry) => entry.test === test && entry.case)
      let transformed = code

      for (const entry of skips) {
        const declaration = `it('${entry.case}'`
        if (!transformed.includes(declaration)) {
          throw new Error(`Ledger case not found in ${entry.test}: ${entry.case}`)
        }
        transformed = transformed.replace(declaration, `it.skip('${entry.case}'`)
      }
      return transformed === code ? null : transformed
    },
  }
}

export default defineConfig({
  plugins: [skipLedgerCases()],
  resolve: {
    alias: [
      { find: /^\.\.\/src\/index(?:\.ts)?$/, replacement: localEntry },
      { find: /^\.\.\/src\/decode\/event-builder(?:\.ts)?$/, replacement: localEntry },
    ],
  },
  test: {
    root,
    include: [`${upstreamTests}/**/*.test.ts`],
    exclude: ledger.entries
      .filter((entry) => entry.case === undefined)
      .map(({ test }) => `${upstreamTests}/${test}`),
  },
})
