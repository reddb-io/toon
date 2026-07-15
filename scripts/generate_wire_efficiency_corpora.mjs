#!/usr/bin/env node
import { writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { serialize } from '../packages/toon/src/index.js'

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)))
const OUTPUT = join(REPO_ROOT, 'tests/wire-efficiency/corpora.json')
const SEED = 0x5eed_0096

function mulberry32(seed) {
  let state = seed >>> 0
  return () => {
    state += 0x6d2b79f5
    let value = state
    value = Math.imul(value ^ (value >>> 15), value | 1)
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61)
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296
  }
}

function pick(random, values) {
  return values[Math.floor(random() * values.length)]
}

function cents(random, min, max) {
  return Math.round((min + random() * (max - min)) * 100) / 100
}

function shipments(count, random) {
  const origins = ['LAX', 'SEA', 'DFW', 'MIA', 'JFK', 'ORD']
  const destinations = ['AMS', 'GRU', 'NRT', 'CDG', 'SCL', 'LHR']
  return {
    shipments: Array.from({ length: count }, (_, index) => ({
      id: `shp_${String(index + 1).padStart(5, '0')}`,
      origin: pick(random, origins),
      destination: pick(random, destinations),
      weightKg: cents(random, 0.4, 180),
      priority: pick(random, ['standard', 'express', 'deferred']),
      insured: random() > 0.42,
    })),
  }
}

function accounts(count, random) {
  const tiers = ['free', 'pro', 'enterprise']
  return {
    accounts: Array.from({ length: count }, (_, index) => ({
      id: `acct_${String(index + 1).padStart(4, '0')}`,
      owner: {
        name: `Owner ${index + 1}`,
        region: pick(random, ['na', 'eu', 'latam', 'apac']),
      },
      plan: {
        tier: pick(random, tiers),
        seats: 1 + Math.floor(random() * 240),
      },
      active: random() > 0.18,
    })),
  }
}

function registry(count, random) {
  const registry = {}
  for (let index = 0; index < count; index += 1) {
    registry[`svc-${String(index + 1).padStart(4, '0')}`] = {
      image: `registry.example/app-${index % 17}`,
      version: `${1 + (index % 4)}.${Math.floor(random() * 20)}.${Math.floor(random() * 50)}`,
      replicas: 1 + Math.floor(random() * 8),
      healthy: random() > 0.09,
    }
  }
  return { registry }
}

function services(count, random) {
  return {
    services: Array.from({ length: count }, (_, index) => ({
      name: `service-${String(index + 1).padStart(4, '0')}`,
      endpoint: {
        host: `api-${index % 23}.example.internal`,
        port: 8000 + (index % 200),
      },
      limits: {
        rps: 100 + Math.floor(random() * 900),
        timeoutMs: pick(random, [250, 500, 750, 1000, 1500]),
      },
      enabled: random() > 0.11,
    })),
  }
}

function tagged(count, random) {
  const tags = ['fragile', 'cold', 'hazmat', 'oversize', 'audit', 'return', 'gift', 'bulk']
  return {
    items: Array.from({ length: count }, (_, index) => ({
      id: `item_${String(index + 1).padStart(4, '0')}`,
      sku: `SKU-${100000 + index}`,
      tags: Array.from({ length: 1 + Math.floor(random() * 4) }, () => pick(random, tags)),
      quantity: 1 + Math.floor(random() * 90),
    })),
  }
}

function matrix(rows, columns, random) {
  return {
    matrix: Array.from({ length: rows }, (_, row) =>
      Array.from({ length: columns }, (_, column) =>
        Math.round((Math.sin(row / 7) + Math.cos(column / 3) + random()) * 1000) / 1000,
      ),
    ),
  }
}

function tree3(count, random) {
  return {
    orders: Array.from({ length: count }, (_, order) => ({
      id: `ord_${String(order + 1).padStart(4, '0')}`,
      customer: `cust_${String(1 + Math.floor(random() * 80)).padStart(3, '0')}`,
      items: Array.from({ length: 1 + Math.floor(random() * 4) }, (_, item) => ({
        sku: `SKU-${order}-${item}`,
        quantity: 1 + Math.floor(random() * 9),
        components: Array.from({ length: 1 + Math.floor(random() * 3) }, (_, component) => ({
          part: `part-${(order + item + component) % 31}`,
          lot: `lot-${Math.floor(random() * 9000)}`,
          ok: random() > 0.07,
        })),
      })),
    })),
  }
}

function nonUniformRows() {
  return {
    rows: [
      { id: 1, point: { x: 1, y: 2 } },
      { id: 2, point: { x: 3, z: 4 } },
    ],
  }
}

function nonUniformMap() {
  return {
    people: {
      ada: { first: 'Ada', last: 'Lovelace' },
      grace: { first: 'Grace', active: true },
    },
  }
}

function compactJsonBytes(value) {
  return Buffer.byteLength(JSON.stringify(value), 'utf8')
}

function toonBytes(value, options = {}) {
  return Buffer.byteLength(serialize(value, options), 'utf8')
}

function caseEntry({ name, description, value, honestyZeroDelta = false, specTokens = undefined }) {
  const expectedBytes = {
    jsonMin: compactJsonBytes(value),
    toonV3: toonBytes(value),
    toonExt: toonBytes(value, { nestedTabularHeaders: true, keyedMapCollapse: true }),
  }
  return {
    name,
    description,
    honestyZeroDelta,
    specTokens,
    expectedBytes,
    value,
  }
}

const random = mulberry32(SEED)
const cases = [
  caseEntry({
    name: 'shipments-500',
    description: 'Flat shipment records; Spec further note baseline includes the tab-delimiter quick win.',
    value: shipments(500, random),
    specTokens: { toonV3: 21578, hypothetical: 21196, tolerancePct: 5 },
  }),
  caseEntry({
    name: 'accounts-300',
    description: 'Uniform nested account records; exercises the shipped nested-tabular extension.',
    value: accounts(300, random),
  }),
  caseEntry({
    name: 'registry-200',
    description: 'Uniform keyed service registry; exercises the shipped keyed-map extension.',
    value: registry(200, random),
  }),
  caseEntry({
    name: 'services-250',
    description: 'Uniform nested service records; a second nested-tabular corpus with operational strings.',
    value: services(250, random),
  }),
  caseEntry({
    name: 'tagged-300',
    description: 'Primitive-array column honesty control; current shipped extensions must not change bytes.',
    value: tagged(300, random),
    honestyZeroDelta: true,
    specTokens: { jsonMin: 6506, toonV3: 8698, hypothetical: 4325, tolerancePct: 5 },
  }),
  caseEntry({
    name: 'matrix-150x8',
    description: 'List-of-lists honesty control; matrix-only hypothetical remains worse than JSON in the Spec.',
    value: matrix(150, 8, random),
    honestyZeroDelta: true,
    specTokens: { jsonMin: 2406, toonV3: 3305, hypothetical: 2707, tolerancePct: 5 },
  }),
  caseEntry({
    name: 'tree3-100',
    description: 'Three-level record tree honesty control for future child-table work.',
    value: tree3(100, random),
    honestyZeroDelta: true,
    specTokens: { jsonMin: 11953, toonV3: 13284, hypothetical: 7484, tolerancePct: 5 },
  }),
  caseEntry({
    name: 'honesty-non-uniform-rows',
    description: 'Nested-tabular fallback control; recursive shape mismatch must stay canonical.',
    value: nonUniformRows(),
    honestyZeroDelta: true,
  }),
  caseEntry({
    name: 'honesty-non-uniform-map',
    description: 'Keyed-map fallback control; non-uniform values must stay canonical.',
    value: nonUniformMap(),
    honestyZeroDelta: true,
  }),
]

const fixture = {
  version: 1,
  seed: `0x${SEED.toString(16)}`,
  generator: 'scripts/generate_wire_efficiency_corpora.mjs',
  encodings: ['jsonMin', 'toonV3', 'toonExt'],
  notes: [
    'jsonMin is JSON.stringify(value).',
    'toonV3 is canonical TOON v3 output with no extensions enabled.',
    'toonExt enables the currently shipped nestedTabularHeaders and keyedMapCollapse options.',
    'honestyZeroDelta cases must keep toonV3 and toonExt byte-identical until a future extension is implemented.',
    'specTokens records the Spec #93 Further Notes o200k_base baselines where available; token counting is local-only and not part of CI.',
  ],
  cases,
}

writeFileSync(OUTPUT, `${JSON.stringify(fixture, null, 2)}\n`)
console.log(`wrote ${OUTPUT}`)
