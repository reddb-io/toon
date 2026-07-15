#!/usr/bin/env node
import { readFileSync, mkdirSync, readdirSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

import { countTokens } from 'gpt-tokenizer'
import { encodeRecords, serialize } from '../../packages/toon/src/index.js'

const REPO_ROOT = dirname(dirname(dirname(fileURLToPath(import.meta.url))))
const RESULTS_DIR = join(REPO_ROOT, 'benchmarks', 'results')
const DATASETS_DIR = join(REPO_ROOT, 'benchmarks', 'datasets')
const REPORT_PATH = join(RESULTS_DIR, '2026-07-15-token-efficiency.md')
const EXTENSIONS = {
  'toon-ext-primitive-array-columns': { primitiveArrayColumns: true },
  'toon-ext-child-tables': { nestedTabularHeaders: true, objectArrayColumns: true },
  'toon-ext-delimiter-pipe': { delimiter: '|' },
  'toon-ext-keyed-map-collapse': { nestedTabularHeaders: true, keyedMapCollapse: true },
  'toon-ext-all': {
    nestedTabularHeaders: true,
    keyedMapCollapse: true,
    primitiveArrayColumns: true,
    objectArrayColumns: true,
  },
}

const encoder = new TextEncoder()

function bytes(value) {
  return encoder.encode(value).length
}

function metric(text) {
  return { bytes: bytes(text), tokens: countTokens(text) }
}

function pct(value, base) {
  if (base === 0) return 'n/a'
  return `${(((value - base) / base) * 100).toFixed(1)}%`
}

function representativeDatasets() {
  const files = datasetFiles(DATASETS_DIR)
  return files.map((file) => {
    const value = JSON.parse(readFileSync(file, 'utf8'))
    const parts = relative(DATASETS_DIR, file).split('/')
    const shapeClass = parts[0]
    const name = parts.join('/').replace(/\.json$/, '')
    const variant = variantName(name)
    return {
      name,
      section: 'representative',
      shapeClass,
      variant,
      recordCount: recordCount(value),
      file,
      value,
      records: firstRecordArray(value),
    }
  })
}

function datasetFiles(dir) {
  return readdirSync(dir)
    .flatMap((entry) => {
      const path = join(dir, entry)
      if (statSync(path).isDirectory()) return datasetFiles(path)
      return path.endsWith('.json') ? [path] : []
    })
    .sort()
}

function localDatasets() {
  const wire = JSON.parse(readFileSync(join(REPO_ROOT, 'tests/corpus/wire-efficiency/corpora.json'), 'utf8'))
  const datasets = wire.cases.map((testCase) => ({
    name: `wire-${testCase.name}`,
    section: 'extension-eligibility showcase',
    shapeClass: 'wire-showcase',
    value: testCase.value,
    records: firstRecordArray(testCase.value),
  }))
  const primitive = JSON.parse(readFileSync(join(REPO_ROOT, 'tests/corpus/wire-efficiency/primitive-array-columns.json'), 'utf8'))
  datasets.push(...primitive.cases.map((testCase) => ({
    name: `wire-extension-${slug(testCase.name)}`,
    section: 'extension-eligibility showcase',
    shapeClass: 'wire-showcase',
    value: testCase.expected,
    records: firstRecordArray(testCase.expected),
  })))
  const objectArray = JSON.parse(readFileSync(join(REPO_ROOT, 'tests/corpus/wire-efficiency/object-array-columns.json'), 'utf8'))
  datasets.push(...objectArray.cases.map((testCase) => ({
    name: `wire-extension-${slug(testCase.name)}`,
    section: 'extension-eligibility showcase',
    shapeClass: 'wire-showcase',
    value: testCase.expected,
    records: firstRecordArray(testCase.expected),
  })))
  return datasets
}

function firstRecordArray(value) {
  if (Array.isArray(value) && value.every(isFlatRecord)) return value
  if (value && typeof value === 'object') {
    for (const item of Object.values(value)) {
      if (Array.isArray(item) && item.every(isFlatRecord)) return item
    }
  }
  return null
}

function variantName(name) {
  const match = name.match(/-(small|large)$/)
  return match ? match[1] : 'base'
}

function recordCount(value) {
  const array = firstArray(value)
  if (array) return array.length
  if (value && typeof value === 'object' && value.entity) return treeNodeCount(value.entity)
  return 1
}

function firstArray(value) {
  if (Array.isArray(value)) return value
  if (value && typeof value === 'object') {
    for (const item of Object.values(value)) {
      if (Array.isArray(item)) return item
    }
  }
  return null
}

function treeNodeCount(value) {
  if (!value || typeof value !== 'object') return 0
  let count = value.id && value.label ? 1 : 0
  if (Array.isArray(value.statements)) {
    for (const statement of value.statements) {
      count += treeNodeCount(statement.value)
    }
  }
  return count
}

function isFlatRecord(value) {
  return value && typeof value === 'object' && !Array.isArray(value) && Object.values(value).every((item) => item === null || typeof item !== 'object')
}

function slug(value) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')
}

function jsonl(records) {
  return records.map((record) => JSON.stringify(record)).join('\n') + '\n'
}

function csv(records) {
  if (!records) return null
  const fields = [...new Set(records.flatMap((record) => Object.keys(record)))].sort()
  const lines = [fields.join(',')]
  for (const record of records) {
    lines.push(fields.map((field) => csvCell(record[field])).join(','))
  }
  return `${lines.join('\n')}\n`
}

function csvCell(value) {
  const text = value === undefined ? '' : String(value)
  return /[",\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text
}

function yaml(value, depth = 0) {
  const indent = '  '.repeat(depth)
  if (Array.isArray(value)) {
    return value.map((item) => `${indent}- ${yamlScalarOrNested(item, depth + 1)}`).join('\n') + '\n'
  }
  if (value && typeof value === 'object') {
    return Object.entries(value).map(([key, item]) => `${indent}${key}: ${yamlScalarOrNested(item, depth + 1)}`).join('\n') + '\n'
  }
  return `${indent}${JSON.stringify(value)}\n`
}

function yamlScalarOrNested(value, depth) {
  if (value && typeof value === 'object') return `\n${yaml(value, depth).trimEnd()}`
  return JSON.stringify(value)
}

function xml(value, name = 'root') {
  if (Array.isArray(value)) return `<${name}>${value.map((item) => xml(item, 'item')).join('')}</${name}>`
  if (value && typeof value === 'object') {
    return `<${name}>${Object.entries(value).map(([key, item]) => xml(item, key)).join('')}</${name}>`
  }
  return `<${name}>${escapeXml(String(value))}</${name}>`
}

function escapeXml(value) {
  return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
}

function formatsFor(dataset) {
  const formats = new Map([
    ['json-minified', JSON.stringify(dataset.value)],
    ['json-pretty', JSON.stringify(dataset.value, null, 2)],
    ['yaml', yaml(dataset.value)],
    ['xml', xml(dataset.value)],
    ['toon-v3.3-canonical', serialize(dataset.value)],
  ])
  if (dataset.file) formats.set('toon-rust-crate-canonical', rustCanonicalToon(dataset.file))
  for (const [name, options] of Object.entries(EXTENSIONS)) {
    formats.set(name, serialize(dataset.value, options))
  }
  if (dataset.records) {
    formats.set('jsonl', jsonl(dataset.records))
    formats.set('csv', csv(dataset.records))
    formats.set('toonl', encodeRecords(dataset.records))
  }
  return formats
}

function rustCanonicalToon(file) {
  const result = spawnSync('cargo', ['run', '--quiet', '-p', 'reddb-io-tq', '--', '-p', 'json', '-o', 'toon', '.', file], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    maxBuffer: 50 * 1024 * 1024,
  })
  if (result.status !== 0) {
    throw new Error(`Rust crate encoder failed for ${relative(REPO_ROOT, file)}:\n${result.stderr || result.stdout}`)
  }
  return result.stdout
}

function renderReport(rows) {
  const command = 'pnpm benchmark:tokens'
  const representativeRows = rows.filter((row) => row.section === 'representative')
  const showcaseRows = rows.filter((row) => row.section !== 'representative')
  const byShape = summarizeByShape(representativeRows)
  const amortization = amortizationByShape(representativeRows)
  const losses = representativeRows.filter((row) => row.format.startsWith('toon') && row.deltaTokens > 0)
  const lines = [
    '# Token Efficiency Benchmark',
    '',
    `Command: \`${command}\``,
    '',
    'Tokenizer: `o200k_base` via `gpt-tokenizer`.',
    '',
    'Representative datasets are vendored under `benchmarks/datasets/` and are read offline. Canonical TOON is measured through both `@reddb-io/toon` and the Rust crate via the shipped `tq` CLI; extension formats are measured through the JS package implementation. Wire fixtures are retained as an extension-eligibility showcase, not as representative payload evidence.',
    '',
    '## Representative Corpus by Shape',
    '',
    '| Shape | Datasets | Best TOON-family median vs JSON | Best non-TOON median vs JSON |',
    '| --- | ---: | ---: | ---: |',
  ]
  for (const summary of byShape) {
    lines.push(`| ${summary.shapeClass} | ${summary.datasets} | ${summary.bestToon} | ${summary.bestOther} |`)
  }
  lines.push(
    '',
    '## Amortization Curve by Shape and Size',
    '',
    'The crossover point is the first measured record count where the best TOON-family format uses no more tokens than minified JSON. `not observed` means both measured sizes still lose to JSON for that shape.',
    '',
    '| Shape | Variant | Record count | JSON minified tokens | Best TOON-family format | Best TOON-family tokens | Tokens vs JSON | Crossover record count |',
    '| --- | --- | ---: | ---: | --- | ---: | ---: | ---: |',
  )
  for (const row of amortization) {
    lines.push(`| ${row.shapeClass} | ${row.variant} | ${row.recordCount} | ${row.jsonTokens} | ${row.bestToonFormat} | ${row.bestToonTokens} | ${row.vsJson} | ${row.crossover} |`)
  }
  lines.push(
    '',
    '## Explicit TOON/TOONL Losses',
    '',
    '| Shape | Dataset | Format | Tokens vs minified JSON |',
    '| --- | --- | --- | ---: |',
  )
  if (losses.length === 0) {
    lines.push('| n/a | n/a | n/a | n/a |')
  } else {
    for (const row of losses) {
      lines.push(`| ${row.shapeClass} | ${row.dataset} | ${row.format} | ${row.vsJson} |`)
    }
  }
  lines.push(
    '',
    '## Representative Dataset Measurements',
    '',
    '| Shape | Dataset | Variant | Record count | Format | Bytes | Tokens | Tokens vs minified JSON |',
    '| --- | --- | --- | ---: | --- | ---: | ---: | ---: |',
  )
  for (const row of representativeRows) {
    lines.push(`| ${row.shapeClass} | ${row.dataset} | ${row.variant} | ${row.recordCount} | ${row.format} | ${row.bytes} | ${row.tokens} | ${row.vsJson} |`)
  }
  lines.push(
    '',
    '## Wire Extension-Eligibility Showcase',
    '',
    'These `wire-*` fixtures exercise opt-in extension behavior and edge cases. They are not representative corpus evidence.',
    '',
    '| Dataset | Format | Bytes | Tokens | Tokens vs minified JSON |',
    '| --- | --- | ---: | ---: | ---: |',
  )
  for (const row of showcaseRows) {
    lines.push(`| ${row.dataset} | ${row.format} | ${row.bytes} | ${row.tokens} | ${row.vsJson} |`)
  }
  lines.push('')
  return `${lines.join('\n')}\n`
}

function summarizeByShape(rows) {
  const shapeClasses = [...new Set(rows.map((row) => row.shapeClass))].sort()
  return shapeClasses.map((shapeClass) => {
    const shapeRows = rows.filter((row) => row.shapeClass === shapeClass)
    const datasets = new Set(shapeRows.map((row) => row.dataset)).size
    const toonRows = shapeRows.filter((row) => row.format.startsWith('toon'))
    const otherRows = shapeRows.filter((row) => !row.format.startsWith('toon') && row.format !== 'json-minified')
    return {
      shapeClass,
      datasets,
      bestToon: bestMedian(toonRows),
      bestOther: bestMedian(otherRows),
    }
  })
}

function amortizationByShape(rows) {
  const shapeClasses = [...new Set(rows.map((row) => row.shapeClass))].sort()
  const output = []
  for (const shapeClass of shapeClasses) {
    const shapeRows = rows.filter((row) => row.shapeClass === shapeClass)
    const variants = [...new Set(shapeRows.map((row) => row.dataset))]
      .map((dataset) => {
        const datasetRows = shapeRows.filter((row) => row.dataset === dataset)
        const json = datasetRows.find((row) => row.format === 'json-minified')
        const toon = datasetRows
          .filter((row) => row.format.startsWith('toon'))
          .sort((left, right) => left.tokens - right.tokens)[0]
        return { dataset, json, toon }
      })
      .filter((item) => item.json && item.toon)
      .sort((left, right) => left.json.recordCount - right.json.recordCount)
    const crossover = variants.find((item) => item.toon.tokens <= item.json.tokens)?.json.recordCount
    for (const item of variants) {
      output.push({
        shapeClass,
        variant: item.json.variant,
        recordCount: item.json.recordCount,
        jsonTokens: item.json.tokens,
        bestToonFormat: item.toon.format,
        bestToonTokens: item.toon.tokens,
        vsJson: pct(item.toon.tokens, item.json.tokens),
        crossover: crossover ?? 'not observed',
      })
    }
  }
  return output
}

function bestMedian(rows) {
  const formats = [...new Set(rows.map((row) => row.format))].sort()
  let best = null
  for (const format of formats) {
    const deltas = rows.filter((row) => row.format === format).map((row) => row.deltaTokens).sort((left, right) => left - right)
    if (deltas.length === 0) continue
    const middle = Math.floor(deltas.length / 2)
    const median = deltas.length % 2 === 0 ? (deltas[middle - 1] + deltas[middle]) / 2 : deltas[middle]
    if (!best || median < best.median) best = { format, median }
  }
  return best ? `${best.format} (${best.median.toFixed(1)}%)` : 'n/a'
}

const datasets = [...representativeDatasets(), ...localDatasets()]
const rows = []
for (const dataset of datasets) {
  const formats = formatsFor(dataset)
  const jsonTokens = metric(formats.get('json-minified')).tokens
  for (const [format, text] of formats) {
    const counts = metric(text)
    const deltaTokens = ((counts.tokens - jsonTokens) / jsonTokens) * 100
    rows.push({
      dataset: dataset.name,
      section: dataset.section,
      shapeClass: dataset.shapeClass,
      variant: dataset.variant ?? 'n/a',
      recordCount: dataset.recordCount ?? 'n/a',
      format,
      bytes: counts.bytes,
      tokens: counts.tokens,
      deltaTokens,
      vsJson: pct(counts.tokens, jsonTokens),
    })
  }
}

mkdirSync(RESULTS_DIR, { recursive: true })
writeFileSync(REPORT_PATH, renderReport(rows))
console.log(`wrote ${REPORT_PATH}`)
