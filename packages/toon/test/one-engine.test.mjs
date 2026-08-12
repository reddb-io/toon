import test from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { extname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

// The one-engine gate. #319 deleted the pre-v4 parser, writer, and header
// machinery in both languages along with the public API that reached them.
// These symbols name that engine, so any of them reappearing in shipped source
// means a second codec came back.

const REPO_ROOT = fileURLToPath(new URL('../../../', import.meta.url))

const SCANNED_ROOTS = [
  'packages/toon/src',
  'packages/toon/dist',
  'crates/toon/src',
  'crates/tq/src',
  'tests/runners',
]

const SCANNED_EXTENSIONS = new Set(['.ts', '.js', '.mjs', '.rs'])

/** Legacy engine symbols, as a source-text pattern and the name it goes by. */
const FORBIDDEN = [
  [/\bparse_legacy\b/, 'Rust parse_legacy'],
  [/\bto_legacy_toon\b/, 'Rust to_legacy_toon'],
  [/\btry_to_legacy_toon\b/, 'Rust try_to_legacy_toon'],
  [/\bLegacyParseOptions\b/, 'Rust LegacyParseOptions'],
  [/\bLegacyEncodeOptions\b/, 'Rust LegacyEncodeOptions'],
  [/\bdetect_truncation_legacy\b/, 'Rust detect_truncation_legacy'],
  [/\bexpand_paths\b/, 'Rust ParseOptions::expand_paths'],
  [/\bTabularArray\b/, 'Rust TabularArray'],
  [/\btoon_parts\//, 'TypeScript toon_parts module directory'],
  [/\bexpandPaths\b/, 'TypeScript expandPaths option'],
  [/from '\.{1,2}\/legacy\.js'/, "TypeScript '../legacy.js' import"],
  [/@reddb-io\/toon\/legacy/, 'the @reddb-io/toon/legacy subpath'],
]

function* sourceFiles(root) {
  const absolute = join(REPO_ROOT, root)
  if (!existsSync(absolute)) return
  const pending = [absolute]
  while (pending.length > 0) {
    const current = pending.pop()
    for (const entry of readdirSync(current)) {
      const path = join(current, entry)
      if (statSync(path).isDirectory()) {
        pending.push(path)
      } else if (SCANNED_EXTENSIONS.has(extname(entry))) {
        yield path
      }
    }
  }
}

test('legacy engine symbols do not come back to either language', () => {
  const offenders = []

  for (const root of SCANNED_ROOTS) {
    for (const path of sourceFiles(root)) {
      const source = readFileSync(path, 'utf8')
      for (const [pattern, name] of FORBIDDEN) {
        if (pattern.test(source)) {
          offenders.push(`${relative(REPO_ROOT, path)}: ${name}`)
        }
      }
    }
  }

  assert.deepEqual(offenders, [], `legacy engine symbols returned:\n  ${offenders.join('\n  ')}`)
})

test('the package ships one codec and no legacy subpath', () => {
  const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))

  assert.deepEqual(Object.keys(manifest.exports), ['.', './node'])
  assert.equal(existsSync(new URL('../dist/legacy.js', import.meta.url)), false)
  assert.equal(existsSync(new URL('../dist/toon.js', import.meta.url)), false)
  assert.equal(existsSync(new URL('../dist/toon_parts', import.meta.url)), false)
  assert.equal(existsSync(new URL('../src/toon_parts', import.meta.url)), false)
})

test('the Rust crate ships one codec and no legacy modules', () => {
  const parts = join(REPO_ROOT, 'crates/toon/src/lib_parts')

  for (const removed of ['parser.rs', 'header_and_scalar.rs', 'encoder.rs']) {
    assert.equal(existsSync(join(parts, removed)), false, `${removed} is the removed engine`)
  }
})
