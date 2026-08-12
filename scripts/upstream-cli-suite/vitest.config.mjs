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
    throw new Error('Every upstream CLI skip needs a test file and rationale')
  }
}

const localEntry = fileURLToPath(
  new URL('../../packages/toon/src/index.ts', import.meta.url),
)
const localManifest = fileURLToPath(
  new URL('../../packages/toon/package.json', import.meta.url),
)
const binHarness = fileURLToPath(new URL('./bin-harness.mjs', import.meta.url))
const upstreamTests = 'vendor/toon/packages/cli/test'
const upstreamTestRoot = fileURLToPath(
  new URL('../../vendor/toon/packages/cli/test/', import.meta.url),
)

function adaptUpstreamCli() {
  return {
    name: 'adapt-upstream-cli',
    enforce: 'pre',
    resolveId(source, importer) {
      if (source === './utils.ts' && importer?.startsWith(upstreamTestRoot)) {
        return binHarness
      }
    },
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

const rustTarget = process.env.TOON_CLI_TARGET === 'rust'
const wholeFileSkips = ledger.entries
  .filter((entry) => entry.case === undefined)
  .map(({ test }) => `${upstreamTests}/${test}`)

export default defineConfig({
  plugins: [adaptUpstreamCli()],
  resolve: {
    alias: [
      { find: /^\.\.\/\.\.\/toon\/src\/index(?:\.ts)?$/, replacement: localEntry },
      { find: /^\.\.\/package\.json$/, replacement: localManifest },
    ],
  },
  test: {
    root,
    include: rustTarget
      ? [`${upstreamTests}/cli-process.test.ts`]
      : [`${upstreamTests}/**/*.test.ts`],
    exclude: wholeFileSkips,
  },
})
