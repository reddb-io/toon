import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..')
const read = (path) => readFileSync(resolve(root, path), 'utf8')

const currentSurfaces = [
  'README.md',
  'packages/toon/README.md',
  'packages/toon/package.json',
  'crates/toon/README.md',
  'crates/toon/Cargo.toml',
  'crates/tq/README.md',
  'crates/tq/Cargo.toml',
  'packages/vscode-toon/README.md',
  'packages/vscode-toon/package.json',
  'packages/vscode-toon/syntaxes/toon.tmLanguage.json',
  'packages/vscode-toon/syntaxes/toonl.tmLanguage.json',
  'benchmarks/README.md',
  'docs/toon-official-spec.md',
  'docs/options-parity-v4.1.1.md',
  'docs/toon-reddb-spec.md',
  'docs/toonl-reddb-spec.md',
  'docs/proposals/README.md',
  'docs/proposals/nested-tabular-headers.md',
  'docs/proposals/keyed-map-collapse.md',
  'docs/proposals/discriminated-heterogeneous-arrays.md',
]

test('current user-facing surfaces describe the v4.1 baseline without retired claims', () => {
  for (const path of currentSurfaces) {
    const source = read(path)
    assert.doesNotMatch(source, /v3\.3/i, `${path} must not present the retired baseline`)
    assert.doesNotMatch(source, /no build step/i, `${path} must not deny the published build`)
    assert.doesNotMatch(source, /compatibility aliases/i, `${path} must not call no-op flags aliases`)
    assert.doesNotMatch(source, /100% conformance/i, `${path} must not make an unscoped parity claim`)
    assert.doesNotMatch(
      source,
      // Anchored on a flag position so section anchors like
      // `#extension-1--nested-tabular-headers` stay linkable.
      /(?:^|[\s`([])--(?:nested-tabular-headers|keyed-map-collapse)\b/m,
      `${path} must not advertise a retired tq switch`,
    )
  }

  for (const path of [
    'scripts/generate_wire_efficiency_corpora.mjs',
    'scripts/wire_efficiency_token_report.mjs',
    'tests/corpus/wire-efficiency/corpora.json',
    'tests/corpus/wire-efficiency/object-array-columns.json',
    'tests/corpus/wire-efficiency/cyclic-discriminated-arrays.json',
    'packages/toon/test/wire-efficiency.test.mjs',
    'tests/runners/rust/toon/wire_efficiency.rs',
  ]) {
    assert.doesNotMatch(
      read(path),
      /toonV3|strictV3Literal|sameAsV3|canonical TOON v3/i,
      `${path} must use version-neutral fixture vocabulary`,
    )
  }
})

test('known upstream issue mappings cannot regress', () => {
  assert.doesNotMatch(read('docs/proposals/delimiter-choice.md'), /(?:spec#|issues\/|pull\/)48/i)
  assert.doesNotMatch(read('docs/proposals/primitive-array-columns.md'), /(?:spec#|issues\/|pull\/)49/i)

  const feedback = read('docs/upstream-feedback.md')
  assert.match(feedback, /spec#48[^\n]*mixed columnar arrays/i)
  assert.match(feedback, /spec#49[^\n]*(?:roadmap|tabular-generalization)/i)
})

test('frontier status is dated and separates every authority state', () => {
  const official = read('docs/toon-official-spec.md')
  assert.match(official, /audited (?:on|at) 2026-08-07/i)
  for (const status of ['released', 'merged', 'draft', 'conflicting', 'rejected', 'userland-only']) {
    assert.match(official, new RegExp(`\\b${status}\\b`, 'i'), `missing ${status} status`)
  }
})

test('migration notes show observable TypeScript and Rust cutovers', () => {
  const migration = read('docs/migration-v4.md')
  assert.match(migration, /TypeScript cutover/i)
  assert.match(migration, /Rust cutover/i)
  // The guide has to say the pre-v4 engine and its API are gone, not offer
  // them as an escape hatch.
  assert.match(migration, /are gone|were removed/)
  assert.match(migration, /parse_legacy/)
  assert.doesNotMatch(migration, /^import .* from '@reddb-io\/toon\/legacy'/m)
  assert.match(migration, /Before[^\n]*After/is)
})

test('the TypeScript TOONL implementation names the unified v0.2 contract', () => {
  for (const path of [
    'packages/toon/src/toonl.ts',
    'packages/toon/dist/toonl.js',
    'packages/toon/dist/toonl.d.ts',
  ]) {
    const source = read(path)
    assert.match(source, /TOONL v0\.2[^\n]*unified/i, `${path} must identify the unified v0.2 contract`)
    assert.match(source, /docs\/toonl-reddb-spec\.md/, `${path} must identify the normative specification`)
  }
})

test('the v4.1.1 options audit covers every pinned library and CLI option', () => {
  const audit = read('docs/options-parity-v4.1.1.md')
  const optionIds = [
    'encode.indentSize',
    'encode.indent',
    'encode.delimiter',
    'encode.replacer',
    'decode.indentSize',
    'decode.indent',
    'decode.strict',
    'cli.output',
    'cli.encode',
    'cli.decode',
    'cli.delimiter',
    'cli.indent',
    'cli.stats',
    'cli.no-strict',
    'cli.verbose',
  ]

  for (const optionId of optionIds) {
    const escaped = optionId.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    assert.match(
      audit,
      new RegExp('^\\| `' + escaped + '` \\|.*\\b(?:identical|fixed-here|ledgered divergence)\\b', 'im'),
      `${optionId} needs an audit classification`,
    )
  }

  assert.match(audit, /a9e6d97eca931379824f3b6a1ba8fbfbda7d3c53/)
  assert.match(audit, /62f16b369408180f1faf1cba7da1b46d1f336f12/)
  assert.match(audit, /issues\/308/)
  assert.match(audit, /issues\/309/)
})

test('benchmark claims point to their public-path reproduction commands', () => {
  const benchmarkIndex = read('benchmarks/README.md')
  for (const command of [
    'pnpm benchmark:accuracy:verify',
    'pnpm benchmark:tokens',
    'pnpm benchmark:performance',
    'cargo bench -p reddb-io-toon',
  ]) {
    assert.match(benchmarkIndex, new RegExp(command.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))
  }

  for (const path of [
    'benchmarks/results/2026-07-15-retrieval-accuracy.md',
    'benchmarks/results/2026-08-06-token-efficiency.md',
    'benchmarks/results/runtime-performance-js.md',
    'benchmarks/results/runtime-performance-rust.md',
  ]) {
    assert.match(read(path), /(?:Command|Generated by|cargo bench)/i, `${path} needs reproduction evidence`)
  }
})

test('relative Markdown links in user-facing docs resolve in a clean checkout', () => {
  const markdownFiles = currentSurfaces.filter((path) => path.endsWith('.md')).concat([
    'CHANGELOG.md',
    'docs/migration-v4.md',
    'docs/upstream-feedback.md',
    'docs/upstream-monitoring.md',
    'docs/proposals/primitive-array-columns.md',
    'docs/proposals/child-tables-and-matrix.md',
    'docs/proposals/depth-guard.md',
    'docs/proposals/delimiter-choice.md',
    'docs/proposals/detect-truncation.md',
    'docs/proposals/cyclic-discriminated-arrays.md',
  ])

  for (const path of markdownFiles) {
    const source = read(path)
    for (const match of source.matchAll(/!?(?:\[[^\]]*\])\(([^)]+)\)/g)) {
      const target = match[1].split('#', 1)[0]
      if (!target || /^(?:https?:|mailto:)/.test(target)) continue
      const resolved = resolve(root, dirname(path), decodeURIComponent(target))
      assert.ok(existsSync(resolved), `${path} links to missing ${target}`)
    }
  }
})
