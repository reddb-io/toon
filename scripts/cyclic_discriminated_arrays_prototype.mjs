#!/usr/bin/env node
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { spawnSync } from 'node:child_process'

import { serialize } from '../packages/toon/src/index.js'

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)))
const TOKENIZER_DIR = join(REPO_ROOT, '.red/tmp/wire-efficiency-tokenizer')
const TOKENIZER_PACKAGE = 'js-tiktoken'
const TABLE_DELIMITER = '|'
const EXT_OPTIONS = {
  nestedTabularHeaders: true,
  keyedMapCollapse: true,
  primitiveArrayColumns: true,
  childTables: true,
}

function lcg(seed) {
  let state = seed >>> 0
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0
    return state / 0x100000000
  }
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

function bytes(value) {
  return Buffer.byteLength(value, 'utf8')
}

function pct(delta, base) {
  return `${((delta / base) * 100).toFixed(1)}%`
}

function pad(value, width) {
  return String(value).padStart(width, ' ')
}

function isObject(value) {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

function isScalar(value) {
  return value === null || ['string', 'number', 'boolean'].includes(typeof value)
}

function encodeCell(value) {
  return JSON.stringify(value)
}

function decodeCell(value) {
  return JSON.parse(value)
}

function encodeKey(key) {
  return encodeURIComponent(key)
}

function decodeKey(key) {
  return decodeURIComponent(key)
}

function flattenValue(value, prefix = '', out = {}) {
  if (Array.isArray(value)) {
    out[`${prefix}.length`] = value.length
    value.forEach((item, index) => flattenValue(item, `${prefix}.${index}`, out))
    return out
  }
  if (isObject(value)) {
    for (const [key, child] of Object.entries(value)) {
      flattenValue(child, prefix ? `${prefix}.${key}` : key, out)
    }
    return out
  }
  out[prefix] = value
  return out
}

function setPath(target, path, value) {
  const parts = path.split('.')
  let cursor = target
  for (let index = 0; index < parts.length; index += 1) {
    const part = parts[index]
    const next = parts[index + 1]
    if (index === parts.length - 1) {
      cursor[part] = value
      continue
    }
    cursor[part] ??= /^\d+$/.test(next) ? [] : {}
    cursor = cursor[part]
  }
}

function inflateValue(flat) {
  const root = {}
  const lengths = []
  for (const [path, value] of Object.entries(flat)) {
    if (path.endsWith('.length')) {
      lengths.push([path.slice(0, -'.length'.length), value])
    } else {
      setPath(root, path, value)
    }
  }
  for (const [path, length] of lengths) {
    const parts = path.split('.')
    let parent = root
    for (let index = 0; index < parts.length - 1; index += 1) parent = parent[parts[index]]
    const key = parts.at(-1)
    const array = Array.from({ length }, (_, index) => parent?.[key]?.[index])
    parent[key] = array
  }
  return root
}

function fieldUnion(rows) {
  const fields = []
  const seen = new Set()
  for (const row of rows) {
    for (const key of Object.keys(flattenValue(row))) {
      if (seen.has(key)) continue
      seen.add(key)
      fields.push(key)
    }
  }
  return fields
}

function tableHeader(name, rows, fields) {
  return `${encodeKey(name)}[${rows.length}${TABLE_DELIMITER}]{${fields.map(encodeKey).join(TABLE_DELIMITER)}}:`
}

function tableRows(rows, fields) {
  return rows.map((row) => {
    const flat = flattenValue(row)
    return `    ${fields.map((field) => encodeCell(flat[field] ?? null)).join(TABLE_DELIMITER)}`
  })
}

function commonPrefixKeys(rows, discriminator) {
  const keys = Object.keys(rows[0]).filter((key) => key !== discriminator)
  const prefix = []
  for (const key of keys) {
    if (!rows.every((row) => Object.prototype.hasOwnProperty.call(row, key))) break
    if (!rows.every((row) => isScalar(row[key]))) break
    prefix.push(key)
  }
  return prefix
}

function findDiscriminator(rows) {
  for (const key of ['type', 'kind', 'event']) {
    if (rows.every((row) => typeof row[key] === 'string')) return key
  }
  return null
}

function cycleOrder(labels) {
  if (labels.length < 12) return null
  for (let size = 2; size <= Math.min(8, Math.floor(labels.length / 3)); size += 1) {
    const cycle = labels.slice(0, size)
    if (new Set(cycle).size < 2) continue
    let matched = 0
    while (matched < labels.length && labels[matched] === cycle[matched % size]) matched += 1
    const repeats = Math.floor(matched / size)
    const tail = labels.slice(repeats * size)
    if (repeats < 3) continue
    if (tail.length > 0 && !tail.every((label, index) => label === cycle[index])) continue
    const raw = labels.map(encodeURIComponent).join(',')
    const encoded = encodeOrder({ cycle, repeats, tail })
    if (encoded.length <= raw.length * 0.4) return { cycle, repeats, tail, encoded }
  }
  return null
}

function encodeOrder(order) {
  const cycle = order.cycle.map(encodeURIComponent).join(',')
  const tail = order.tail.length === 0 ? '' : `+tail(${order.tail.map(encodeURIComponent).join(',')})`
  return `cycle(${cycle})*${order.repeats}${tail}`
}

function decodeOrder(encoded) {
  const match = encoded.match(/^cycle\((.*)\)\*(\d+)(?:\+tail\((.*)\))?$/)
  assert(match, `bad order grammar: ${encoded}`)
  const cycle = match[1].split(',').map(decodeURIComponent)
  const repeats = Number(match[2])
  const tail = match[3] ? match[3].split(',').map(decodeURIComponent) : []
  return [
    ...Array.from({ length: cycle.length * repeats }, (_, index) => cycle[index % cycle.length]),
    ...tail,
  ]
}

function eligibleSection(rows) {
  if (!Array.isArray(rows) || rows.length === 0 || !rows.every(isObject)) return null
  const discriminator = findDiscriminator(rows)
  if (!discriminator) return null
  const labels = rows.map((row) => row[discriminator])
  const order = cycleOrder(labels)
  if (!order) return null
  const common = commonPrefixKeys(rows, discriminator)
  const groups = new Map()
  for (const row of rows) {
    const label = row[discriminator]
    if (!groups.has(label)) groups.set(label, [])
    const payload = {}
    for (const [key, value] of Object.entries(row)) {
      if (key !== discriminator && !common.includes(key)) payload[key] = value
    }
    groups.get(label).push(payload)
  }
  return { discriminator, common, order, groups }
}

function shippedCyclicWire(value) {
  const section = eligibleSection(value.events)
  if (!section) return JSON.stringify(value)

  const lines = [
    '@toon-cyclic-discriminated-array/1',
    '@root {"events":"$C0"}',
    `@array $C0 discr=${section.discriminator} n=${value.events.length} common=${section.common.join(',')} order=${section.order.encoded}`,
  ]
  if (section.common.length > 0) {
    lines.push('@common')
    for (const row of value.events) lines.push(section.common.map((key) => encodeCell(row[key])).join('\t'))
  }
  for (const [label, rows] of section.groups) {
    lines.push(`@group ${encodeURIComponent(label)} n=${rows.length}`)
    for (const row of rows) lines.push(encodeCell(row))
  }
  lines.push('@end')
  return `${lines.join('\n')}\n`
}

function decodeShippedCyclicWire(wire) {
  if (!wire.startsWith('@toon-cyclic-discriminated-array/1\n')) return JSON.parse(wire)
  const lines = wire.trimEnd().split('\n')
  assert.equal(lines.shift(), '@toon-cyclic-discriminated-array/1')
  assert.equal(lines.shift(), '@root {"events":"$C0"}')
  const header = lines.shift()
  const match = header.match(/^@array \$C0 discr=([^ ]+) n=(\d+) common=([^ ]*) order=(.*)$/)
  assert(match, `bad array header: ${header}`)
  const [, discriminator, nText, commonText, orderText] = match
  const n = Number(nText)
  const common = commonText ? commonText.split(',') : []
  const commonRows = []
  if (lines[0] === '@common') {
    lines.shift()
    for (let i = 0; i < n; i += 1) {
      const cells = lines.shift().split('\t').map(decodeCell)
      commonRows.push(Object.fromEntries(common.map((key, index) => [key, cells[index]])))
    }
  }
  const groups = new Map()
  while (lines[0] !== '@end') {
    const groupHeader = lines.shift()
    const groupMatch = groupHeader.match(/^@group ([^ ]+) n=(\d+)$/)
    assert(groupMatch, `bad group header: ${groupHeader}`)
    const [, labelText, groupSizeText] = groupMatch
    const label = decodeURIComponent(labelText)
    groups.set(label, [])
    for (let i = 0; i < Number(groupSizeText); i += 1) groups.get(label).push(JSON.parse(lines.shift()))
  }
  assert.equal(lines.shift(), '@end')
  const cursors = new Map([...groups.keys()].map((label) => [label, 0]))
  const events = decodeOrder(orderText).map((label, index) => {
    const group = groups.get(label)
    assert(group, `missing group for ${label}`)
    const cursor = cursors.get(label)
    cursors.set(label, cursor + 1)
    return { [discriminator]: label, ...commonRows[index], ...group[cursor] }
  })
  return { events }
}

function genuineToonWire(value) {
  const section = eligibleSection(value.events)
  if (!section) return JSON.stringify(value)

  const commonRows = value.events.map((row) => Object.fromEntries(section.common.map((key) => [key, row[key]])))
  const commonFields = fieldUnion(commonRows)
  const lines = [
    'events:',
    `  order: ${section.order.encoded}`,
    `  discriminator: ${section.discriminator}`,
    `  rows: ${value.events.length}`,
  ]
  if (commonFields.length > 0) {
    lines.push(`  ${tableHeader('common', commonRows, commonFields)}`)
    lines.push(...tableRows(commonRows, commonFields))
  }
  for (const [label, rows] of section.groups) {
    const fields = fieldUnion(rows)
    lines.push(`  ${tableHeader(label, rows, fields)}`)
    lines.push(...tableRows(rows, fields))
  }
  return `${lines.join('\n')}\n`
}

function parseTableHeader(line) {
  const match = line.match(/^  ([^\[]+)\[(\d+)\|\]\{([^}]*)\}:$/)
  assert(match, `bad TOON table header: ${line}`)
  const [, labelText, countText, fieldsText] = match
  return {
    label: decodeKey(labelText),
    count: Number(countText),
    fields: fieldsText ? fieldsText.split(TABLE_DELIMITER).map(decodeKey) : [],
  }
}

function decodeGenuineToonWire(wire) {
  if (!wire.startsWith('events:\n')) return JSON.parse(wire)
  const lines = wire.trimEnd().split('\n')
  assert.equal(lines.shift(), 'events:')
  const order = lines.shift().match(/^  order: (.*)$/)?.[1]
  const discriminator = lines.shift().match(/^  discriminator: (.*)$/)?.[1]
  const rows = Number(lines.shift().match(/^  rows: (\d+)$/)?.[1])
  assert(order, 'missing order')
  assert(discriminator, 'missing discriminator')
  assert(Number.isSafeInteger(rows), 'bad row count')

  let commonRows = []
  const groups = new Map()
  while (lines.length > 0) {
    const header = parseTableHeader(lines.shift())
    const tableRows = []
    for (let index = 0; index < header.count; index += 1) {
      const cells = lines.shift().trimStart().split(TABLE_DELIMITER).map(decodeCell)
      const flat = Object.fromEntries(header.fields.map((field, fieldIndex) => [field, cells[fieldIndex]]))
      tableRows.push(inflateValue(flat))
    }
    if (header.label === 'common') commonRows = tableRows
    else groups.set(header.label, tableRows)
  }

  assert.equal(decodeOrder(order).length, rows)
  const cursors = new Map([...groups.keys()].map((label) => [label, 0]))
  const events = decodeOrder(order).map((label, index) => {
    const group = groups.get(label)
    assert(group, `missing group for ${label}`)
    const cursor = cursors.get(label)
    cursors.set(label, cursor + 1)
    return { [discriminator]: label, ...commonRows[index], ...group[cursor] }
  })
  return { events }
}

function eventFor(type, index, rand, mix) {
  const base = {
    type,
    id: `evt_${String(index).padStart(5, '0')}`,
    ts: `2026-07-15T${String(8 + Math.floor(index / 60)).padStart(2, '0')}:${String(index % 60).padStart(2, '0')}:00Z`,
  }
  if (mix !== 'minimal') base.actor = `user_${1 + Math.floor(rand() * 7)}`
  if (type === 'open') return { ...base, issue: `ISS-${1000 + index}`, priority: ['low', 'medium', 'high'][index % 3] }
  if (type === 'comment') return { ...base, comment: `comment ${index}`, mentions: Math.floor(rand() * 4) }
  if (type === 'check') return { ...base, check: ['lint', 'test', 'build'][index % 3], duration_ms: 1200 + Math.floor(rand() * 9000) }
  if (type === 'deploy') return { ...base, env: ['staging', 'prod'][index % 2], sha: Math.floor(rand() * 0xffffffff).toString(16).padStart(8, '0') }
  return { ...base, metric: Number((rand() * 100).toFixed(3)) }
}

function cyclicCase(name, cycle, records, mix, seed) {
  const rand = lcg(seed)
  return {
    name,
    expectedEligible: true,
    value: { events: Array.from({ length: records }, (_, index) => eventFor(cycle[index % cycle.length], index, rand, mix)) },
  }
}

function controlCase(name, labels, seed) {
  const rand = lcg(seed)
  return {
    name,
    expectedEligible: false,
    value: { events: labels.map((label, index) => eventFor(label, index, rand, 'rich')) },
  }
}

function datasetCase(name, path, expectedEligible) {
  return {
    name,
    expectedEligible,
    value: JSON.parse(readFileSync(join(REPO_ROOT, path), 'utf8')),
  }
}

function shuffle(labels, seed) {
  const rand = lcg(seed)
  const out = [...labels]
  for (let i = out.length - 1; i > 0; i -= 1) {
    const j = Math.floor(rand() * (i + 1))
    ;[out[i], out[j]] = [out[j], out[i]]
  }
  return out
}

const CASES = [
  datasetCase('tagged-records-small', 'benchmarks/datasets/tagged-records/activity-events-small.json', false),
  datasetCase('tagged-records-large', 'benchmarks/datasets/tagged-records/activity-events-large.json', true),
  cyclicCase('cycle2-24-minimal', ['open', 'comment'], 24, 'minimal', 11),
  cyclicCase('cycle3-90-rich', ['open', 'comment', 'check'], 90, 'rich', 17),
  cyclicCase('cycle4-240-rich', ['open', 'comment', 'check', 'deploy'], 240, 'rich', 23),
  cyclicCase('cycle5-500-rich', ['open', 'comment', 'check', 'deploy', 'metric'], 500, 'rich', 29),
  controlCase('control-non-cyclic-irregular', ['open', 'open', 'comment', 'check', 'open', 'deploy', 'comment', 'metric', 'check', 'check', 'deploy', 'open'], 31),
  controlCase('control-partial-cycle', [...Array.from({ length: 8 }, (_, index) => ['open', 'comment', 'check'][index % 3]), 'deploy'], 37),
  controlCase('control-random-types', shuffle(Array.from({ length: 80 }, (_, index) => ['open', 'comment', 'check', 'deploy'][index % 4]), 41), 43),
]

function measure(encoding, testCase) {
  const jsonMin = JSON.stringify(testCase.value)
  const toonV33 = serialize(testCase.value)
  const toonExt = serialize(testCase.value, EXT_OPTIONS)
  const bestCurrent = bytes(toonExt) < bytes(toonV33) ? toonExt : toonV33
  const section = eligibleSection(testCase.value.events)
  const shippedWire = shippedCyclicWire(testCase.value)
  const genuineWire = genuineToonWire(testCase.value)
  const shippedDecoded = decodeShippedCyclicWire(shippedWire)
  const genuineDecoded = decodeGenuineToonWire(genuineWire)
  assert.equal(Boolean(section), testCase.expectedEligible, `${testCase.name}: eligibility mismatch`)
  assert.equal(JSON.stringify(shippedDecoded), jsonMin, `${testCase.name}: shipped round trip`)
  assert.equal(JSON.stringify(genuineDecoded), jsonMin, `${testCase.name}: genuine TOON round trip`)
  if (!section) {
    assert.equal(shippedWire, jsonMin, `${testCase.name}: ineligible shipped control must use JSON fallback`)
    assert.equal(genuineWire, jsonMin, `${testCase.name}: ineligible genuine control must use JSON fallback`)
  } else {
    assert(!genuineWire.includes('@toon-'), `${testCase.name}: genuine TOON wire must not use @toon directive`)
    assert(!genuineWire.includes('$C0'), `${testCase.name}: genuine TOON wire must not use $ references`)
    assert(!genuineWire.split('\n').some((line) => line.trimStart().startsWith('{')), `${testCase.name}: genuine TOON wire must not use JSON object payload lines`)
  }

  return {
    name: testCase.name,
    eligible: Boolean(section),
    records: testCase.value.events.length,
    cycle: section ? section.order.cycle.length : 0,
    repeat: section ? section.order.repeats : 0,
    bytes: {
      jsonMin: bytes(jsonMin),
      toonV33: bytes(toonV33),
      bestCurrent: bytes(bestCurrent),
      shipped: bytes(shippedWire),
      genuine: bytes(genuineWire),
    },
    tokens: {
      jsonMin: encoding.encode(jsonMin).length,
      toonV33: encoding.encode(toonV33).length,
      bestCurrent: encoding.encode(bestCurrent).length,
      shipped: encoding.encode(shippedWire).length,
      genuine: encoding.encode(genuineWire).length,
    },
    sample: section ? genuineWire : null,
  }
}

function printReport(results) {
  console.log('Cyclic discriminated arrays genuine-TOON prototype (o200k_base)')
  console.log('')
  console.log(
    [
      'Corpus'.padEnd(30),
      'Elig',
      pad('Rows', 5),
      pad('Cycle', 5),
      pad('Rep', 5),
      pad('JSON b', 8),
      pad('TOON b', 8),
      pad('Best b', 8),
      pad('Ship b', 8),
      pad('Genu b', 8),
      pad('Genu vs ship', 12),
      pad('JSON tok', 9),
      pad('TOON tok', 9),
      pad('Best tok', 9),
      pad('Ship tok', 9),
      pad('Genu tok', 9),
      pad('Genu vs ship', 12),
    ].join('  '),
  )
  console.log('-'.repeat(175))
  for (const result of results) {
    console.log(
      [
        result.name.padEnd(30),
        result.eligible ? 'yes ' : 'no  ',
        pad(result.records, 5),
        pad(result.cycle || '-', 5),
        pad(result.repeat || '-', 5),
        pad(result.bytes.jsonMin, 8),
        pad(result.bytes.toonV33, 8),
        pad(result.bytes.bestCurrent, 8),
        pad(result.bytes.shipped, 8),
        pad(result.bytes.genuine, 8),
        pad(pct(result.bytes.genuine - result.bytes.shipped, result.bytes.shipped), 12),
        pad(result.tokens.jsonMin, 9),
        pad(result.tokens.toonV33, 9),
        pad(result.tokens.bestCurrent, 9),
        pad(result.tokens.shipped, 9),
        pad(result.tokens.genuine, 9),
        pad(pct(result.tokens.genuine - result.tokens.shipped, result.tokens.shipped), 12),
      ].join('  '),
    )
  }
  console.log('')
  console.log('Round trip: every eligible shipped and genuine-TOON wire decodes to byte-identical minified JSON.')
  console.log('Controls: non-cyclic irregular, partial-cycle, and random sequences are ineligible and use lossless JSON fallback.')
  console.log('Genuine TOON: eligible wires use nested metadata plus tabular common/group sub-tables; no @ directives, JSON object payload lines, or $ references.')

  if (process.argv.includes('--samples')) {
    for (const result of results.filter((item) => item.sample).slice(0, 2)) {
      console.log('')
      console.log(`Sample — ${result.name}`)
      console.log(result.sample.trimEnd())
    }
  }
}

const { getEncoding } = await ensureTokenizer()
const encoding = getEncoding('o200k_base')
const results = CASES.map((testCase) => measure(encoding, testCase))
if (process.argv.includes('--check')) {
  assert(results.some((result) => result.eligible))
  assert(results.some((result) => !result.eligible))
}
printReport(results)
