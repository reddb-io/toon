#!/usr/bin/env node
import assert from 'node:assert/strict'
import { existsSync, readFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { createRequire } from 'node:module'
import { spawnSync } from 'node:child_process'

import { serialize } from '../packages/toon/src/index.js'

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)))
const TOKENIZER_DIR = join(REPO_ROOT, '.red/tmp/wire-efficiency-tokenizer')
const TOKENIZER_PACKAGE = 'js-tiktoken'
const EXT_OPTIONS = { nestedTabularHeaders: true, keyedMapCollapse: true }
const DATASETS = [
  {
    corpus: 'tagged-records',
    variant: 'small',
    path: 'benchmarks/datasets/tagged-records/activity-events-small.json',
  },
  {
    corpus: 'tagged-records',
    variant: 'large',
    path: 'benchmarks/datasets/tagged-records/activity-events-large.json',
  },
  {
    corpus: 'nested-heterogeneous',
    variant: 'small',
    path: 'benchmarks/datasets/nested-heterogeneous/json-schema-event-small.json',
  },
  {
    corpus: 'nested-heterogeneous',
    variant: 'large',
    path: 'benchmarks/datasets/nested-heterogeneous/json-schema-event-large.json',
  },
]

const EXPECTED = {
  'tagged-records/small': {
    bytes: { jsonMin: 697, toonV33: 759, bestCurrent: 759, candidateC: 654, candidateB: 839 },
    tokens: { jsonMin: 216, toonV33: 258, bestCurrent: 258, candidateC: 248, candidateB: 271 },
  },
  'tagged-records/large': {
    bytes: { jsonMin: 20360, toonV33: 22191, bestCurrent: 22191, candidateC: 16491, candidateB: 17906 },
    tokens: { jsonMin: 6386, toonV33: 7632, bestCurrent: 7632, candidateC: 6302, candidateB: 5864 },
  },
  'nested-heterogeneous/small': {
    bytes: { jsonMin: 1621, toonV33: 1966, bestCurrent: 1966, candidateC: 1737, candidateB: 1805 },
    tokens: { jsonMin: 447, toonV33: 509, bestCurrent: 509, candidateC: 502, candidateB: 521 },
  },
  'nested-heterogeneous/large': {
    bytes: { jsonMin: 28378, toonV33: 29963, bestCurrent: 29963, candidateC: 28105, candidateB: 29500 },
    tokens: { jsonMin: 8459, toonV33: 9405, bestCurrent: 9405, candidateC: 8592, candidateB: 9157 },
  },
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
    if (result.status !== 0) process.exit(result.status ?? 1)
  }

  const requireFromTokenizerDir = createRequire(join(TOKENIZER_DIR, 'noop.cjs'))
  return import(pathToFileURL(requireFromTokenizerDir.resolve(TOKENIZER_PACKAGE)))
}

function byteLength(value) {
  return Buffer.byteLength(value, 'utf8')
}

function pct(value, base) {
  return `${((value / base) * 100).toFixed(1)}%`
}

function pad(value, width) {
  return String(value).padStart(width, ' ')
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

function isScalar(value) {
  return value === null || ['string', 'number', 'boolean'].includes(typeof value)
}

function commonPrefixKeys(rows) {
  if (rows.length === 0) return []
  const firstKeys = Object.keys(rows[0])
  const prefix = []
  for (const key of firstKeys) {
    if (!rows.every((row) => Object.prototype.hasOwnProperty.call(row, key))) break
    if (!rows.every((row) => isScalar(row[key]))) break
    prefix.push(key)
  }
  return prefix
}

function objectKeySignature(row) {
  return Object.keys(row).join('\u0000')
}

function looksHeterogeneousObjectArray(value) {
  if (!Array.isArray(value) || value.length === 0) return false
  if (!value.every(isPlainObject)) return false
  const signatures = new Set(value.map(objectKeySignature))
  if (signatures.size > 1) return true
  return value.some((row) => Object.values(row).some((cellValue) => Array.isArray(cellValue) || isPlainObject(cellValue)))
}

function findCandidateArrays(value, path = []) {
  const here = []
  if (looksHeterogeneousObjectArray(value) && commonPrefixKeys(value).length > 0) {
    here.push({ path, rows: value })
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => here.push(...findCandidateArrays(item, path.concat(String(index)))))
  } else if (isPlainObject(value)) {
    for (const [key, child] of Object.entries(value)) {
      here.push(...findCandidateArrays(child, path.concat(key)))
    }
  }
  return here
}

function pathId(path) {
  return path.join('.')
}

function cloneWithPlaceholders(value, targets, prefix) {
  const byPath = new Map(targets.map((target, index) => [pathId(target.path), `${prefix}${index}`]))
  function visit(node, path) {
    const placeholder = byPath.get(pathId(path))
    if (placeholder) return placeholder
    if (Array.isArray(node)) return node.map((item, index) => visit(item, path.concat(String(index))))
    if (isPlainObject(node)) {
      const out = {}
      for (const [key, child] of Object.entries(node)) out[key] = visit(child, path.concat(key))
      return out
    }
    return node
  }
  return visit(value, [])
}

function encodeCell(value) {
  return JSON.stringify(value)
}

function decodeCell(value) {
  return JSON.parse(value)
}

function splitCellLine(line) {
  return line.length === 0 ? [] : line.split('\t').map(decodeCell)
}

function candidateCRows(rows) {
  const prefix = commonPrefixKeys(rows)
  return {
    prefix,
    rows: rows.map((row) => {
      const payload = {}
      for (const [key, value] of Object.entries(row)) {
        if (!prefix.includes(key)) payload[key] = value
      }
      return [...prefix.map((key) => row[key]), payload]
    }),
  }
}

function candidateCWire(value) {
  const targets = findCandidateArrays(value)
  const root = cloneWithPlaceholders(value, targets, '$C')
  const lines = ['@toon-hyp-c/1', `@root ${JSON.stringify(root)}`]
  targets.forEach((target, index) => {
    const section = candidateCRows(target.rows)
    lines.push(`@array $C${index} path=${pathId(target.path)} n=${target.rows.length} prefix=${section.prefix.join(',')}`)
    for (const row of section.rows) lines.push(row.map(encodeCell).join('\t'))
    lines.push('@end')
  })
  return `${lines.join('\n')}\n`
}

function decodeCandidateC(wire) {
  const lines = wire.trimEnd().split('\n')
  assert.equal(lines.shift(), '@toon-hyp-c/1')
  const root = JSON.parse(lines.shift().slice('@root '.length))
  const replacements = new Map()
  while (lines.length > 0) {
    const header = lines.shift()
    const match = header.match(/^@array (\$C\d+) path=.* n=(\d+) prefix=(.*)$/)
    assert(match, `bad C header: ${header}`)
    const [, id, nText, prefixText] = match
    const prefix = prefixText ? prefixText.split(',') : []
    const rows = []
    for (let i = 0; i < Number(nText); i += 1) {
      const cells = splitCellLine(lines.shift())
      const payload = cells[prefix.length]
      const row = {}
      prefix.forEach((key, index) => {
        row[key] = cells[index]
      })
      for (const [key, value] of Object.entries(payload)) row[key] = value
      rows.push(row)
    }
    assert.equal(lines.shift(), '@end')
    replacements.set(id, rows)
  }
  return replacePlaceholders(root, replacements)
}

function discriminatorForRows(rows) {
  const prefix = commonPrefixKeys(rows)
  if (prefix.includes('type')) return { key: 'type', label: (row) => row.type, omitKey: 'type' }
  if (prefix.includes('kind')) return { key: 'kind', label: (row) => row.kind, omitKey: 'kind' }
  if (rows.every((row) => row.properties?.kind?.const)) {
    return { key: 'properties.kind.const', label: (row) => row.properties.kind.const, omitKey: null }
  }
  return { key: prefix[0], label: (row) => String(row[prefix[0]]), omitKey: prefix[0] }
}

function encodeOrder(labels) {
  if (labels.length === 0) return '[]'
  for (let size = 1; size <= Math.floor(labels.length / 2); size += 1) {
    if (labels.length % size !== 0) continue
    const cycle = labels.slice(0, size)
    if (labels.every((label, index) => label === cycle[index % size])) {
      return `cycle(${cycle.map(encodeURIComponent).join(',')})*${labels.length / size}`
    }
  }
  return labels.map(encodeURIComponent).join(',')
}

function decodeOrder(encoded) {
  if (encoded === '[]') return []
  const cycle = encoded.match(/^cycle\((.*)\)\*(\d+)$/)
  if (cycle) {
    const labels = cycle[1].length === 0 ? [] : cycle[1].split(',').map(decodeURIComponent)
    const repeat = Number(cycle[2])
    return Array.from({ length: labels.length * repeat }, (_, index) => labels[index % labels.length])
  }
  return encoded.split(',').map(decodeURIComponent)
}

function candidateBSections(rows) {
  const discriminator = discriminatorForRows(rows)
  const labels = rows.map(discriminator.label)
  const groups = new Map()
  for (const row of rows) {
    const label = discriminator.label(row)
    if (!groups.has(label)) groups.set(label, [])
    const out = {}
    for (const [key, value] of Object.entries(row)) {
      if (key !== discriminator.omitKey) out[key] = value
    }
    groups.get(label).push(out)
  }
  return { discriminator, order: encodeOrder(labels), groups }
}

function candidateBWire(value) {
  const targets = findCandidateArrays(value)
  const root = cloneWithPlaceholders(value, targets, '$B')
  const lines = ['@toon-hyp-b/1', `@root ${JSON.stringify(root)}`]
  targets.forEach((target, index) => {
    const section = candidateBSections(target.rows)
    lines.push(
      `@array $B${index} path=${pathId(target.path)} discr=${section.discriminator.key} omit=${section.discriminator.omitKey ?? '-'} order=${section.order}`,
    )
    for (const [label, groupRows] of section.groups) {
      lines.push(`@group ${encodeURIComponent(label)} n=${groupRows.length}`)
      for (const row of groupRows) lines.push(encodeCell(row))
    }
    lines.push('@end')
  })
  return `${lines.join('\n')}\n`
}

function decodeCandidateB(wire) {
  const lines = wire.trimEnd().split('\n')
  assert.equal(lines.shift(), '@toon-hyp-b/1')
  const root = JSON.parse(lines.shift().slice('@root '.length))
  const replacements = new Map()
  while (lines.length > 0) {
    const header = lines.shift()
    const match = header.match(/^@array (\$B\d+) path=.* discr=([^ ]+) omit=([^ ]+) order=(.*)$/)
    assert(match, `bad B header: ${header}`)
    const [, id, , omitText, orderText] = match
    const omitKey = omitText === '-' ? null : omitText
    const grouped = new Map()
    while (lines[0] !== '@end') {
      const groupHeader = lines.shift()
      const groupMatch = groupHeader.match(/^@group ([^ ]+) n=(\d+)$/)
      assert(groupMatch, `bad B group header: ${groupHeader}`)
      const [, labelText, nText] = groupMatch
      const label = decodeURIComponent(labelText)
      grouped.set(label, [])
      for (let i = 0; i < Number(nText); i += 1) grouped.get(label).push(JSON.parse(lines.shift()))
    }
    lines.shift()
    const cursors = new Map([...grouped.keys()].map((label) => [label, 0]))
    const rows = decodeOrder(orderText).map((label) => {
      const group = grouped.get(label)
      const index = cursors.get(label)
      cursors.set(label, index + 1)
      const row = {}
      if (omitKey) row[omitKey] = label
      for (const [key, value] of Object.entries(group[index])) row[key] = value
      return row
    })
    replacements.set(id, rows)
  }
  return replacePlaceholders(root, replacements)
}

function replacePlaceholders(value, replacements) {
  if (typeof value === 'string' && replacements.has(value)) return replacements.get(value)
  if (Array.isArray(value)) return value.map((item) => replacePlaceholders(item, replacements))
  if (isPlainObject(value)) {
    const out = {}
    for (const [key, child] of Object.entries(value)) out[key] = replacePlaceholders(child, replacements)
    return out
  }
  return value
}

function measure(encoding, dataset) {
  const value = JSON.parse(readFileSync(join(REPO_ROOT, dataset.path), 'utf8'))
  const jsonMin = JSON.stringify(value)
  const toonV33 = serialize(value)
  const toonExt = serialize(value, EXT_OPTIONS)
  const bestCurrentWire = byteLength(toonExt) < byteLength(toonV33) ? toonExt : toonV33
  const cWire = candidateCWire(value)
  const bWire = candidateBWire(value)

  assert.equal(JSON.stringify(decodeCandidateC(cWire)), jsonMin, `${dataset.corpus}/${dataset.variant}: C round trip`)
  assert.equal(JSON.stringify(decodeCandidateB(bWire)), jsonMin, `${dataset.corpus}/${dataset.variant}: B round trip`)

  return {
    key: `${dataset.corpus}/${dataset.variant}`,
    candidateCount: findCandidateArrays(value).length,
    bytes: {
      jsonMin: byteLength(jsonMin),
      toonV33: byteLength(toonV33),
      bestCurrent: byteLength(bestCurrentWire),
      candidateC: byteLength(cWire),
      candidateB: byteLength(bWire),
    },
    tokens: {
      jsonMin: encoding.encode(jsonMin).length,
      toonV33: encoding.encode(toonV33).length,
      bestCurrent: encoding.encode(bestCurrentWire).length,
      candidateC: encoding.encode(cWire).length,
      candidateB: encoding.encode(bWire).length,
    },
  }
}

function assertExpected(results) {
  for (const result of results) {
    assert.deepEqual(result.bytes, EXPECTED[result.key].bytes, `${result.key}: byte measurements drifted`)
    assert.deepEqual(result.tokens, EXPECTED[result.key].tokens, `${result.key}: token measurements drifted`)
  }
}

function printReport(results) {
  console.log('Discriminated / heterogeneous array prototype (o200k_base)')
  console.log('')
  console.log(
    [
      'Corpus'.padEnd(22),
      'Arrays',
      pad('JSON b', 8),
      pad('TOON b', 8),
      pad('Best b', 8),
      pad('C b', 8),
      pad('B b', 8),
      pad('C vs JSON', 10),
      pad('B vs JSON', 10),
      pad('JSON tok', 9),
      pad('TOON tok', 9),
      pad('Best tok', 9),
      pad('C tok', 9),
      pad('B tok', 9),
      pad('C vs JSON', 10),
      pad('B vs JSON', 10),
    ].join('  '),
  )
  console.log('-'.repeat(171))
  for (const result of results) {
    console.log(
      [
        result.key.padEnd(22),
        pad(result.candidateCount, 6),
        pad(result.bytes.jsonMin, 8),
        pad(result.bytes.toonV33, 8),
        pad(result.bytes.bestCurrent, 8),
        pad(result.bytes.candidateC, 8),
        pad(result.bytes.candidateB, 8),
        pad(pct(result.bytes.candidateC - result.bytes.jsonMin, result.bytes.jsonMin), 10),
        pad(pct(result.bytes.candidateB - result.bytes.jsonMin, result.bytes.jsonMin), 10),
        pad(result.tokens.jsonMin, 9),
        pad(result.tokens.toonV33, 9),
        pad(result.tokens.bestCurrent, 9),
        pad(result.tokens.candidateC, 9),
        pad(result.tokens.candidateB, 9),
        pad(pct(result.tokens.candidateC - result.tokens.jsonMin, result.tokens.jsonMin), 10),
        pad(pct(result.tokens.candidateB - result.tokens.jsonMin, result.tokens.jsonMin), 10),
      ].join('  '),
    )
  }
  console.log('')
  console.log('Round trip: candidate C and candidate B decoded back to byte-identical minified JSON for every row above.')
}

const { getEncoding } = await ensureTokenizer()
const encoding = getEncoding('o200k_base')
const results = DATASETS.map((dataset) => measure(encoding, dataset))

if (process.argv.includes('--check')) assertExpected(results)
printReport(results)
