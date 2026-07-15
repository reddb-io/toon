#!/usr/bin/env node
import { existsSync, readFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { createRequire } from 'node:module'
import { spawnSync } from 'node:child_process'

import { serialize } from '../packages/toon/src/index.js'

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)))
const FIXTURE_PATH = join(REPO_ROOT, 'tests/wire-efficiency/corpora.json')
const TOKENIZER_DIR = join(REPO_ROOT, '.red/tmp/wire-efficiency-tokenizer')
const TOKENIZER_PACKAGE = 'js-tiktoken'
const EXT_OPTIONS = {
  nestedTabularHeaders: true,
  keyedMapCollapse: true,
  primitiveArrayColumns: true,
  objectArrayColumns: true,
}

function ensureTokenizer() {
  const packageJson = join(TOKENIZER_DIR, 'node_modules', TOKENIZER_PACKAGE, 'package.json')
  if (!existsSync(packageJson)) {
    mkdirSync(TOKENIZER_DIR, { recursive: true })
    const result = spawnSync(
      'npm',
      ['install', '--silent', '--no-audit', '--no-fund', '--prefix', TOKENIZER_DIR, TOKENIZER_PACKAGE],
      { stdio: 'inherit' },
    )
    if (result.status !== 0) {
      process.exit(result.status ?? 1)
    }
  }

  const requireFromTokenizerDir = createRequire(join(TOKENIZER_DIR, 'noop.cjs'))
  return import(pathToFileURL(requireFromTokenizerDir.resolve(TOKENIZER_PACKAGE)))
}

function tokens(encoding, value) {
  return encoding.encode(value).length
}

function pct(delta, base) {
  return `${((delta / base) * 100).toFixed(1)}%`
}

function pad(value, width) {
  return String(value).padStart(width, ' ')
}

const { getEncoding } = await ensureTokenizer()
const encoding = getEncoding('o200k_base')
const fixture = JSON.parse(readFileSync(FIXTURE_PATH, 'utf8'))

console.log(`Wire-efficiency token report (${fixture.seed}, o200k_base)`)
console.log('')
console.log(
  [
    'Scenario'.padEnd(26),
    pad('JSON min', 9),
    pad('TOON v3', 9),
    pad('TOON tab', 9),
    pad('TOON+ext', 9),
    pad('ext vs JSON', 11),
    'Spec baseline',
  ].join('  '),
)
console.log('-'.repeat(93))

for (const testCase of fixture.cases) {
  const value = testCase.value
  const jsonMin = JSON.stringify(value)
  const toonV3 = serialize(value)
  const toonTab = serialize(value, { delimiter: '\t' })
  const toonExt = serialize(value, EXT_OPTIONS)
  const counts = {
    jsonMin: tokens(encoding, jsonMin),
    toonV3: tokens(encoding, toonV3),
    toonTab: tokens(encoding, toonTab),
    toonExt: tokens(encoding, toonExt),
  }
  const spec = testCase.specTokens
    ? `JSON ${testCase.specTokens.jsonMin ?? '-'} / TOON ${testCase.specTokens.toonV3} / hyp ${testCase.specTokens.hypothetical}`
    : '-'
  console.log(
    [
      testCase.name.padEnd(26),
      pad(counts.jsonMin, 9),
      pad(counts.toonV3, 9),
      pad(counts.toonTab, 9),
      pad(counts.toonExt, 9),
      pad(pct(counts.toonExt - counts.jsonMin, counts.jsonMin), 11),
      spec,
    ].join('  '),
  )
}
