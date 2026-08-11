import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { encode } from '../dist/index.js'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '..', '..', '..')
const corpusDir = join(root, 'tests', 'corpus', 'encoder-parity')
const seedConfig = JSON.parse(readFileSync(join(corpusDir, 'seeds.json'), 'utf8'))
const regressions = JSON.parse(readFileSync(join(corpusDir, 'regressions.json'), 'utf8'))
const STRING_PARTS = [
  '', 'true', 'null', '42', 'a,b', 'a:b', '# hash', ' padded ', '"quote"',
  '\\slash', '\t', '\n', 'á', '中', '🙂', '[brackets]', '{braces}',
]

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

function integer(random, minimum, maximum) {
  return minimum + Math.floor(random() * (maximum - minimum + 1))
}

function pick(random, values) {
  return values[integer(random, 0, values.length - 1)]
}

function randomString(random) {
  if (random() < 0.65) return pick(random, STRING_PARTS)
  return Array.from(
    { length: integer(random, 0, 12) },
    () => String.fromCharCode(integer(random, 32, 126)),
  ).join('')
}

function randomScalar(random) {
  const roll = random()
  if (roll < 0.15) return null
  if (roll < 0.3) return random() < 0.5
  if (roll < 0.55) return integer(random, -100000, 100000)
  if (roll < 0.7) return integer(random, -100000, 100000) / 10
  return randomString(random)
}

function randomKey(random, index) {
  if (random() < 0.6) return `field_${index}_${integer(random, 0, 99)}`
  return randomString(random)
}

function randomValue(random, depth) {
  if (depth === 0 || random() < 0.45) return randomScalar(random)
  if (random() < 0.45) {
    return Array.from(
      { length: integer(random, 0, 4) },
      () => randomValue(random, depth - 1),
    )
  }
  const value = {}
  const fields = integer(random, 0, 4)
  for (let index = 0; index < fields; index += 1) {
    value[randomKey(random, index)] = randomValue(random, depth - 1)
  }
  return value
}

function generate(seed, iterations) {
  const random = mulberry32(seed)
  return Array.from({ length: iterations }, () => randomValue(random, 4))
}

function iterations() {
  const requested = process.env.TOON_ENCODER_PARITY_ITERATIONS
  if (requested === undefined) return seedConfig.iterations
  if (!/^\d+$/.test(requested) || Number(requested) < 1) {
    throw new Error('TOON_ENCODER_PARITY_ITERATIONS must be a positive integer')
  }
  return Math.max(seedConfig.iterations, Number(requested))
}

function rustEncode(values) {
  const input = values.map((value) => JSON.stringify(value)).join('\n') + '\n'
  const result = spawnSync(
    'cargo',
    ['run', '--quiet', '-p', 'reddb-io-toon', '--example', 'encoder_parity'],
    { cwd: root, input, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 },
  )
  assert.equal(result.status, 0, `Rust parity runner failed:\n${result.stderr}`)
  const lines = result.stdout.trimEnd() === '' ? [] : result.stdout.trimEnd().split('\n')
  assert.equal(lines.length, values.length, 'Rust parity runner returned the wrong number of cases')
  return lines.map((line) => JSON.parse(line))
}

function * shrinkCandidates(value) {
  if (Array.isArray(value)) {
    yield []
    for (let index = 0; index < value.length; index += 1) {
      yield [...value.slice(0, index), ...value.slice(index + 1)]
      for (const smaller of shrinkCandidates(value[index])) {
        const candidate = [...value]
        candidate[index] = smaller
        yield candidate
      }
    }
    return
  }
  if (value !== null && typeof value === 'object') {
    const entries = Object.entries(value)
    yield {}
    for (const [key, nested] of entries) {
      yield Object.fromEntries(entries.filter(([candidate]) => candidate !== key))
      yield nested
      for (const smaller of shrinkCandidates(nested)) {
        yield Object.fromEntries(entries.map(([candidate, item]) => [
          candidate,
          candidate === key ? smaller : item,
        ]))
      }
    }
    return
  }
  if (typeof value === 'string' && value !== '') {
    yield ''
    if (value.length > 1) {
      yield value.slice(0, Math.ceil(value.length / 2))
      yield value.slice(Math.floor(value.length / 2))
    }
  } else if (typeof value === 'number' && value !== 0) {
    yield 0
  } else if (value === true) {
    yield false
  }
}

function shrink(value, stillFails) {
  let smallest = value
  let changed = true
  while (changed) {
    changed = false
    for (const candidate of shrinkCandidates(smallest)) {
      if (stillFails(candidate)) {
        smallest = candidate
        changed = true
        break
      }
    }
  }
  return smallest
}

test('synthetic counterexamples shrink to the committed minimal fixture', () => {
  for (const fixture of regressions) {
    const minimal = shrink(
      fixture.original,
      (candidate) => JSON.stringify(candidate).includes('mismatch'),
    )
    assert.deepEqual(minimal, fixture.minimal, fixture.name)
  }
})

test('committed seeds produce byte-identical Rust and TypeScript encodings', () => {
  const count = iterations()
  for (const seed of seedConfig.seeds) {
    const values = generate(seed, count)
    assert.deepEqual(generate(seed, count), values, `seed ${seed} is not deterministic`)
    const rustWires = rustEncode(values)
    for (const [index, value] of values.entries()) {
      const tsWire = encode(value)
      if (rustWires[index] === tsWire) continue
      const minimal = shrink(value, (candidate) => rustEncode([candidate])[0] !== encode(candidate))
      assert.equal(
        rustWires[index],
        tsWire,
        `seed ${seed}, case ${index}; commit this minimal fixture:\n${JSON.stringify(minimal)}`,
      )
    }
  }
})
