import { readFileSync, writeFileSync } from 'node:fs'
import { isDeepStrictEqual } from 'node:util'

const DELIMITERS = [',', '\t', '|']
const EDGE_NUMBERS = [
  -0,
  0,
  Number.MAX_SAFE_INTEGER,
  Number.MAX_SAFE_INTEGER + 1,
  -Number.MAX_SAFE_INTEGER - 1,
  1e-7,
  1e21,
  5e-324,
  1.7976931348623157e308,
]
const EDGE_STRINGS = [
  '',
  '-0',
  '1e+21',
  'true',
  'null',
  'comma,value',
  'pipe|value',
  'tab\tvalue',
  'colon: value',
  'brackets[]{}',
  'quote"slash\\',
  'line\nbreak',
  'nul\0unit\u001f',
  'e\u0301',
  '中🙂',
]
const EDGE_KEYS = ['', ...EDGE_STRINGS.slice(4), ' spaced key ', 'a.b', 'a,b|c\td:e']

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

function integer(random, maximum) {
  return Math.floor(random() * maximum)
}

function generatedString(random) {
  if (random() < 0.7) return pick(random, EDGE_STRINGS)
  return Array.from(
    { length: integer(random, 13) },
    () => String.fromCodePoint(pick(random, [32 + integer(random, 95), 0x301, 0x4e2d, 0x1f642])),
  ).join('')
}

function generatedScalar(random) {
  const choice = integer(random, 5)
  if (choice === 0) return null
  if (choice === 1) return random() < 0.5
  if (choice === 2) return pick(random, EDGE_NUMBERS)
  if (choice === 3) return (random() - 0.5) * 10 ** integer(random, 23)
  return generatedString(random)
}

function generatedValue(random, depth) {
  if (depth === 0 || random() < 0.42) return generatedScalar(random)
  if (random() < 0.45) {
    return Array.from({ length: integer(random, 5) }, () => generatedValue(random, depth - 1))
  }

  const entries = []
  const length = integer(random, 5)
  for (let index = 0; index < length; index += 1) {
    const key = random() < 0.75 ? pick(random, EDGE_KEYS) : generatedString(random)
    entries.push([key, generatedValue(random, depth - 1)])
  }
  return Object.fromEntries(entries)
}

function outcome(decode, wire) {
  try {
    return { accepted: true, value: decode(wire) }
  } catch {
    return { accepted: false }
  }
}

function mutations(wire) {
  const candidates = [
    ['append-newline', `${wire}\n`],
    ['truncate', wire.length === 0 ? ':' : wire.slice(0, -1)],
    ['remove-key-space', wire.replace(': ', ':')],
    ['damage-indent', wire.replace('\n  ', '\n ')],
    ['increment-array-count', wire.replace(/\[(\d+)\]/, (_, count) => `[${Number(count) + 1}]`)],
    ['remove-first-quote', wire.replace('"', '')],
    ['append-malformed-array', `${wire}\ninvalid[1]{x}:\n  value`],
  ]
  const seen = new Set()
  return candidates
    .filter(([, candidate]) => candidate !== wire && !seen.has(candidate) && seen.add(candidate))
    .map(([mutation, candidate]) => ({ mutation, wire: candidate }))
}

function matchesLedger(divergence, entry) {
  return divergence.direction === entry.signature.direction
    && divergence.mutation === entry.signature.mutation
    && divergence.local?.accepted === entry.signature.localAccepted
    && divergence.upstream?.accepted === entry.signature.upstreamAccepted
}

function findDivergence(value, implementations, ledgerEntries = []) {
  for (const delimiter of DELIMITERS) {
    let localWire
    let upstreamWire
    try {
      localWire = implementations.local.encode(value, { delimiter })
      upstreamWire = implementations.upstream.encode(value, { delimiter })
    } catch (error) {
      return { direction: 'encode-threw', delimiter, detail: error.message }
    }
    if (localWire !== upstreamWire) {
      return { direction: 'encode-bytes', delimiter, localWire, upstreamWire }
    }

    for (const candidate of mutations(localWire)) {
      const local = outcome(implementations.local.decode, candidate.wire)
      const upstream = outcome(implementations.upstream.decode, candidate.wire)
      if (!isDeepStrictEqual(local, upstream)) {
        const divergence = {
          direction: 'decode-mutation',
          mutation: candidate.mutation,
          delimiter,
          mutated: candidate.wire,
          local,
          upstream,
        }
        if (!ledgerEntries.some(entry => matchesLedger(divergence, entry))) return divergence
      }
    }
  }
  return undefined
}

function immediateShrinks(value) {
  if (value === null) return []
  if (typeof value === 'boolean') return value ? [false] : []
  if (typeof value === 'number') return Object.is(value, -0) ? [0] : [0, -0]
  if (typeof value === 'string') return value === '' ? [] : ['', value.slice(0, Math.floor(value.length / 2))]
  if (Array.isArray(value)) {
    const candidates = [[], value.slice(0, Math.floor(value.length / 2))]
    for (let index = 0; index < value.length; index += 1) {
      candidates.push(value.toSpliced(index, 1))
      for (const child of immediateShrinks(value[index])) candidates.push(value.with(index, child))
    }
    return candidates
  }

  const entries = Object.entries(value)
  const candidates = [{}]
  for (let index = 0; index < entries.length; index += 1) {
    candidates.push(Object.fromEntries(entries.toSpliced(index, 1)))
    const [key, childValue] = entries[index]
    for (const child of immediateShrinks(childValue)) {
      candidates.push(Object.fromEntries(entries.with(index, [key, child])))
    }
  }
  return candidates
}

function shrink(value, implementations, ledgerEntries) {
  let smallest = value
  for (let attempt = 0; attempt < 200; attempt += 1) {
    const smaller = immediateShrinks(smallest)
      .find(candidate => findDivergence(candidate, implementations, ledgerEntries))
    if (smaller === undefined) break
    smallest = smaller
  }
  return smallest
}

function fixtureJson(value) {
  if (value === null || typeof value === 'boolean') return String(value)
  if (typeof value === 'number') return Object.is(value, -0) ? '-0' : String(value)
  if (typeof value === 'string') return JSON.stringify(value)
  if (Array.isArray(value)) return `[${value.map(fixtureJson).join(',')}]`
  return `{${Object.entries(value).map(([key, child]) => `${JSON.stringify(key)}:${fixtureJson(child)}`).join(',')}}`
}

function failWithCounterexample(value, implementations, ledgerEntries, context) {
  const smallest = shrink(value, implementations, ledgerEntries)
  const divergence = findDivergence(smallest, implementations, ledgerEntries)
  const fixture = {
    schemaVersion: 1,
    issue: 302,
    description: 'Minimized differential counterexample',
    json: fixtureJson(smallest),
  }
  if (process.env.TOON_DIFFERENTIAL_FIXTURE) {
    writeFileSync(process.env.TOON_DIFFERENTIAL_FIXTURE, `${JSON.stringify(fixture, null, 2)}\n`)
  }
  throw new Error(`${context}\nminimal fixture:\n${JSON.stringify(fixture, null, 2)}\ndivergence:\n${JSON.stringify(divergence, null, 2)}`)
}

export function exerciseFixture(fixture, implementations) {
  if (fixture.schemaVersion !== 1 || typeof fixture.json !== 'string') {
    throw new Error('differential fixture must have schemaVersion 1 and a JSON source string')
  }
  const value = JSON.parse(fixture.json)
  const ledgerEntries = readAndVerifyLedger(implementations)
  const divergence = findDivergence(value, implementations, ledgerEntries)
  if (divergence) failWithCounterexample(value, implementations, ledgerEntries, fixture.description)
}

function readAndVerifyLedger(implementations) {
  const ledger = JSON.parse(readFileSync(new URL('../differential-divergences.json', import.meta.url), 'utf8'))
  if (ledger.schemaVersion !== 1 || !ledger.ratchetRule?.includes('Remove')) {
    throw new Error('differential ledger must state its removal ratchet')
  }

  const ids = new Set()
  for (const entry of ledger.entries) {
    if (ids.has(entry.id)) throw new Error(`duplicate differential ledger id: ${entry.id}`)
    ids.add(entry.id)
    const value = JSON.parse(entry.probe.json)
    const wire = implementations.local.encode(value, { delimiter: entry.probe.delimiter })
    const candidate = mutations(wire).find(item => item.mutation === entry.probe.mutation)
    if (!candidate) throw new Error(`ledger probe ${entry.id} did not produce its mutation`)
    const divergence = {
      direction: 'decode-mutation',
      mutation: candidate.mutation,
      delimiter: entry.probe.delimiter,
      mutated: candidate.wire,
      local: outcome(implementations.local.decode, candidate.wire),
      upstream: outcome(implementations.upstream.decode, candidate.wire),
    }
    if (!matchesLedger(divergence, entry) || isDeepStrictEqual(divergence.local, divergence.upstream)) {
      throw new Error(`stale differential ledger entry must be removed: ${entry.id}`)
    }
  }
  return ledger.entries
}

export function runDifferentialFuzz(implementations, options) {
  if (!Number.isSafeInteger(options.cases) || options.cases < 1) throw new Error('cases must be a positive integer')
  if (!Number.isSafeInteger(options.timeBudgetMs) || options.timeBudgetMs < 1) throw new Error('time budget must be positive')

  const random = mulberry32(options.seed)
  const ledgerEntries = readAndVerifyLedger(implementations)
  const deadline = performance.now() + options.timeBudgetMs
  let casesRun = 0
  while (casesRun < options.cases && (casesRun === 0 || performance.now() < deadline)) {
    const value = generatedValue(random, 5)
    const divergence = findDivergence(value, implementations, ledgerEntries)
    if (divergence) {
      failWithCounterexample(value, implementations, ledgerEntries, `seed ${options.seed}, case ${casesRun}`)
    }
    casesRun += 1
  }
  return { casesRun, seed: options.seed }
}
