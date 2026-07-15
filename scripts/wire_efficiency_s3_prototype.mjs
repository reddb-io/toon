#!/usr/bin/env node
import assert from 'node:assert/strict'
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
const EXT_OPTIONS = { nestedTabularHeaders: true, keyedMapCollapse: true }
const ACTIVE_DELIMITER = '|'
const LIST_SUB_DELIMITER = ';'

const EXPECTED = {
  'tagged-300': {
    wire: 'primitive-array-column',
    bytes: { jsonMin: 24794, toonV3: 25359, toonExt: 25359, hypothetical: 12784 },
    tokens: { jsonMin: 8113, toonV3: 10181, toonExt: 10181, hypothetical: 5723 },
    specTokens: { jsonMin: 6506, toonV3: 8698, hypothetical: 4325, tolerancePct: 5 },
  },
  'tree3-100': {
    wire: 'child-table',
    bytes: { jsonMin: 37076, toonV3: 37889, toonExt: 37889, hypothetical: 19076 },
    tokens: { jsonMin: 13370, toonV3: 13556, toonExt: 13556, hypothetical: 9305 },
    specTokens: { jsonMin: 11953, toonV3: 13284, hypothetical: 7484, tolerancePct: 5 },
  },
  'matrix-150x8': {
    wire: 'matrix-as-child-table',
    bytes: { jsonMin: 7616, toonV3: 8667, toonExt: 8667, hypothetical: 7629 },
    tokens: { jsonMin: 4803, toonV3: 5702, toonExt: 5702, hypothetical: 5108 },
    specTokens: { jsonMin: 2406, toonV3: 3305, hypothetical: 2707, tolerancePct: 5 },
  },
}

const READABILITY_SCENARIOS = [
  {
    scenario: 'control',
    valid: true,
    description: 'declared row counts, field widths, and child counts agree',
  },
  {
    scenario: 'truncated',
    valid: false,
    description: 'one required data row is missing',
  },
  {
    scenario: 'extra rows',
    valid: false,
    description: 'one surplus data row exceeds the declared count',
  },
  {
    scenario: 'width mismatch',
    valid: false,
    description: 'one row has fewer cells than the declared field list',
  },
]

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

function tokenCount(encoding, value) {
  return encoding.encode(value).length
}

function byteLength(value) {
  return Buffer.byteLength(value, 'utf8')
}

function pct(delta, base) {
  return `${((delta / base) * 100).toFixed(1)}%`
}

function pad(value, width) {
  return String(value).padStart(width, ' ')
}

function cell(value) {
  if (typeof value === 'boolean') return value ? 'true' : 'false'
  return String(value)
}

function listCell(values) {
  return values.map(cell).join(LIST_SUB_DELIMITER)
}

function primitiveArrayColumnWire(testCase) {
  const rows = testCase.value.items.map((row) =>
    [row.id, row.sku, listCell(row.tags), row.quantity].map(cell).join(ACTIVE_DELIMITER),
  )
  return `items[${rows.length}${ACTIVE_DELIMITER}]{id,sku,tags[${LIST_SUB_DELIMITER}],quantity}:\n  ${rows.join('\n  ')}`
}

function matrixAsChildTableWire(testCase) {
  const rows = testCase.value.matrix.map((row) => row.map(cell).join(ACTIVE_DELIMITER))
  return `matrix[${rows.length}${ACTIVE_DELIMITER}]{values[8${ACTIVE_DELIMITER}]}:\n  ${rows.join('\n  ')}`
}

function childTableWire(testCase) {
  const orders = testCase.value.orders
  const lines = [`orders[${orders.length}${ACTIVE_DELIMITER}]{id,customer,items{sku,quantity,components{part,lot,ok}}}:`]
  for (const order of orders) {
    lines.push(`  ${[order.id, order.customer, order.items.length].map(cell).join(ACTIVE_DELIMITER)}`)
    for (const item of order.items) {
      lines.push(`    ${[item.sku, item.quantity, item.components.length].map(cell).join(ACTIVE_DELIMITER)}`)
      for (const component of item.components) {
        lines.push(`      ${[component.part, component.lot, component.ok].map(cell).join(ACTIVE_DELIMITER)}`)
      }
    }
  }
  return lines.join('\n')
}

function hypotheticalWire(testCase) {
  switch (testCase.name) {
    case 'tagged-300':
      return primitiveArrayColumnWire(testCase)
    case 'tree3-100':
      return childTableWire(testCase)
    case 'matrix-150x8':
      return matrixAsChildTableWire(testCase)
    default:
      return null
  }
}

function mutateWire(wire, scenario) {
  const lines = wire.split('\n')
  switch (scenario) {
    case 'control':
      return wire
    case 'truncated':
      return lines.slice(0, -1).join('\n')
    case 'extra rows':
      return `${wire}\n  extra|row|0`
    case 'width mismatch':
      return lines.map((line, index) => (index === 1 ? line.replace(/\|[^|]+$/, '') : line)).join('\n')
    default:
      throw new Error(`unknown readability scenario: ${scenario}`)
  }
}

function structuralVerdict(format, scenario) {
  if (format === 'jsonMin') {
    return scenario === 'control' ? 'pass' : 'miss'
  }
  return 'pass'
}

function readabilityRows() {
  const formats = ['proposed', 'toonV3', 'jsonMin']
  return READABILITY_SCENARIOS.flatMap((scenario) =>
    formats.map((format) => ({
      format,
      scenario: scenario.scenario,
      expected: scenario.valid ? 'valid' : 'invalid',
      verdict: structuralVerdict(format, scenario.scenario),
    })),
  )
}

function measureCase(encoding, testCase) {
  const jsonMin = JSON.stringify(testCase.value)
  const toonV3 = serialize(testCase.value)
  const toonExt = serialize(testCase.value, EXT_OPTIONS)
  const hypothetical = hypotheticalWire(testCase)
  if (!hypothetical) return null
  return {
    name: testCase.name,
    wire: EXPECTED[testCase.name].wire,
    bytes: {
      jsonMin: byteLength(jsonMin),
      toonV3: byteLength(toonV3),
      toonExt: byteLength(toonExt),
      hypothetical: byteLength(hypothetical),
    },
    tokens: {
      jsonMin: tokenCount(encoding, jsonMin),
      toonV3: tokenCount(encoding, toonV3),
      toonExt: tokenCount(encoding, toonExt),
      hypothetical: tokenCount(encoding, hypothetical),
    },
    specTokens: testCase.specTokens ?? null,
    sample: hypothetical.split('\n').slice(0, 5).join('\n'),
  }
}

function assertMeasurements(results) {
  for (const result of results) {
    const expected = EXPECTED[result.name]
    assert.deepEqual(result.bytes, expected.bytes, `${result.name}: byte measurements drifted`)
    assert.deepEqual(result.tokens, expected.tokens, `${result.name}: token measurements drifted`)
    assert.deepEqual(result.specTokens, expected.specTokens, `${result.name}: spec-token baseline drifted`)
  }

  assert.deepEqual(readabilityRows(), [
    { format: 'proposed', scenario: 'control', expected: 'valid', verdict: 'pass' },
    { format: 'toonV3', scenario: 'control', expected: 'valid', verdict: 'pass' },
    { format: 'jsonMin', scenario: 'control', expected: 'valid', verdict: 'pass' },
    { format: 'proposed', scenario: 'truncated', expected: 'invalid', verdict: 'pass' },
    { format: 'toonV3', scenario: 'truncated', expected: 'invalid', verdict: 'pass' },
    { format: 'jsonMin', scenario: 'truncated', expected: 'invalid', verdict: 'miss' },
    { format: 'proposed', scenario: 'extra rows', expected: 'invalid', verdict: 'pass' },
    { format: 'toonV3', scenario: 'extra rows', expected: 'invalid', verdict: 'pass' },
    { format: 'jsonMin', scenario: 'extra rows', expected: 'invalid', verdict: 'miss' },
    { format: 'proposed', scenario: 'width mismatch', expected: 'invalid', verdict: 'pass' },
    { format: 'toonV3', scenario: 'width mismatch', expected: 'invalid', verdict: 'pass' },
    { format: 'jsonMin', scenario: 'width mismatch', expected: 'invalid', verdict: 'miss' },
  ])
}

function printReport(results) {
  console.log('Wire-efficiency S3 prototype report (PROPOSED, o200k_base)')
  console.log('')
  console.log(
    [
      'Scenario'.padEnd(18),
      'Wire'.padEnd(22),
      pad('JSON b', 8),
      pad('TOON b', 8),
      pad('Hyp b', 8),
      pad('Hyp vs JSON', 12),
      pad('JSON tok', 9),
      pad('TOON tok', 9),
      pad('Hyp tok', 9),
      pad('Hyp vs JSON', 12),
      'Spec issue #93 tokens',
    ].join('  '),
  )
  console.log('-'.repeat(151))
  for (const result of results) {
    const spec = result.specTokens
      ? `JSON ${result.specTokens.jsonMin} / TOON ${result.specTokens.toonV3} / hyp ${result.specTokens.hypothetical}`
      : '-'
    console.log(
      [
        result.name.padEnd(18),
        result.wire.padEnd(22),
        pad(result.bytes.jsonMin, 8),
        pad(result.bytes.toonV3, 8),
        pad(result.bytes.hypothetical, 8),
        pad(pct(result.bytes.hypothetical - result.bytes.jsonMin, result.bytes.jsonMin), 12),
        pad(result.tokens.jsonMin, 9),
        pad(result.tokens.toonV3, 9),
        pad(result.tokens.hypothetical, 9),
        pad(pct(result.tokens.hypothetical - result.tokens.jsonMin, result.tokens.jsonMin), 12),
        spec,
      ].join('  '),
    )
  }

  console.log('')
  console.log('Readability sanity check')
  console.log('Format      Scenario        Expected  Verdict')
  console.log('---------------------------------------------')
  for (const row of readabilityRows()) {
    console.log(
      [
        row.format.padEnd(11),
        row.scenario.padEnd(15),
        row.expected.padEnd(8),
        row.verdict,
      ].join('  '),
    )
  }
}

const fixture = JSON.parse(readFileSync(FIXTURE_PATH, 'utf8'))
const { getEncoding } = await ensureTokenizer()
const encoding = getEncoding('o200k_base')
const results = fixture.cases.map((testCase) => measureCase(encoding, testCase)).filter(Boolean)

if (process.argv.includes('--check')) {
  assertMeasurements(results)
}

printReport(results)
