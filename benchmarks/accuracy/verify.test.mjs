import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { decode, encode } from '../../packages/toon/dist/index.js'
import {
  evaluateGenerationAttempts,
  renderAccuracyReportSnapshot,
  validateAccuracyReport,
} from './generation.mjs'
import {
  createBenchmarkSuite,
  encodeBenchmarkDocuments,
} from './suite.mjs'

const FIXTURES_DIR = join(dirname(fileURLToPath(import.meta.url)), 'fixtures')

function readFixture(name) {
  return JSON.parse(readFileSync(join(FIXTURES_DIR, name), 'utf8'))
}

test('dataset and question generation is deterministic', () => {
  const first = createBenchmarkSuite()
  const second = createBenchmarkSuite()

  assert.deepEqual(first, second)
  assert.equal(first.seed, 218)
  assert.equal(first.datasets.length, 5)
  assert.equal(first.questions.length, 13)
  assert.equal(first.generationTasks.length, 2)
  assert.deepEqual(first.datasets[0].data.employees[0], {
    id: 'emp-001',
    name: 'Ada Lovelace',
    department: 'Operations',
    salary: 67218,
    yearsExperience: 3,
    active: false,
  })
  assert.deepEqual(
    [...new Set(first.questions.map((question) => question.style))].sort(),
    ['structural-corruption', 'structured-question'],
  )
})

test('generation tasks and validators are deterministic without an LLM', () => {
  const fixture = readFixture('structured-generation.json')
  const first = createBenchmarkSuite()
  const second = createBenchmarkSuite()
  const taskById = new Map(first.generationTasks.map((task) => [task.id, task]))
  const decoders = { json: JSON.parse, toon: decode }

  assert.deepEqual(first.generationTasks, second.generationTasks)
  assert.deepEqual(
    fixture.cases.map(({ taskId, format, promptBudget }) => ({ taskId, format, promptBudget })),
    first.generationTasks.flatMap((task) => ['json', 'toon'].map((format) => ({
      taskId: task.id,
      format,
      promptBudget: task.promptBudget,
    }))),
  )

  const evaluated = fixture.cases.map((entry) => evaluateGenerationAttempts({
    task: taskById.get(entry.taskId),
    format: entry.format,
    attempts: entry.attempts,
    decode: decoders[entry.format],
  }))

  assert.ok(evaluated.every((entry) => entry.syntaxValid && entry.semanticValid))
  assert.ok(evaluated.some((entry) => entry.retries > 0))
  assert.ok(evaluated.flatMap((entry) => entry.attempts).some((attempt) => !attempt.syntaxValid))
  assert.ok(evaluated.flatMap((entry) => entry.attempts).some((attempt) =>
    attempt.syntaxValid && !attempt.semanticValid))
  assert.deepEqual(
    evaluated.flatMap((entry) => entry.attempts).map(({ raw, rawArtifactRef }) => ({ raw, rawArtifactRef })),
    fixture.cases.flatMap((entry) => entry.attempts).map(({ raw, rawArtifactRef }) => ({ raw, rawArtifactRef })),
  )
})

test('JSON and TOON generation fixtures use identical tasks and prompt budgets', () => {
  const fixture = readFixture('structured-generation.json')
  const comparable = (format) => fixture.cases
    .filter((entry) => entry.format === format)
    .map(({ taskId, promptBudget }) => ({ taskId, promptBudget }))

  assert.deepEqual(comparable('json'), comparable('toon'))
})

test('accuracy report schema fixture carries reproducibility and observation evidence', () => {
  const report = readFixture('report-schema.json')

  assert.doesNotThrow(() => validateAccuracyReport(report))
  for (const field of [
    'model',
    'prompt',
    'format',
    'retries',
    'tokens',
    'syntaxValid',
    'semanticAccuracy',
    'provenance',
    'rawArtifactRefs',
  ]) {
    const missing = structuredClone(report)
    delete missing.observations[0][field]
    assert.throws(() => validateAccuracyReport(missing), new RegExp(field))
  }
})

test('report snapshot separates offline verification from non-CI model observations', () => {
  const report = readFixture('report-schema.json')
  const expected = readFileSync(join(FIXTURES_DIR, 'report-snapshot.md'), 'utf8')

  assert.equal(renderAccuracyReportSnapshot(report), expected)
  assert.match(expected, /Offline verification \(reproducible\)/)
  assert.match(expected, /Model observations \(non-CI\)/)
})

test('questions are derived from the generated data', () => {
  const suite = createBenchmarkSuite()
  const employees = suite.datasets[0].data.employees
  const byId = new Map(suite.questions.map((question) => [question.id, question]))

  assert.equal(byId.get('employees-count')?.expected, employees.length)
  assert.equal(
    byId.get('employees-engineering-count')?.expected,
    employees.filter((employee) => employee.department === 'Engineering').length,
  )
  assert.equal(
    byId.get('employees-active-high-earners')?.expected,
    employees.filter((employee) => employee.active && employee.salary >= 90000).length,
  )
})

test('post-encode corruption preserves TOON declarations and changes rows', () => {
  const suite = createBenchmarkSuite()
  const documents = encodeBenchmarkDocuments(suite, [
    { id: 'toon-js', format: 'toon', encode },
    { id: 'json-compact', format: 'json', encode: JSON.stringify },
  ])
  const find = (encoderId, datasetId) =>
    documents.find((document) =>
      document.encoderId === encoderId && document.datasetId === datasetId)

  const control = find('toon-js', 'structural-control').text
  const truncated = find('toon-js', 'structural-truncated').text
  const extra = find('toon-js', 'structural-extra-rows').text
  const width = find('toon-js', 'structural-width-mismatch').text
  const declaredCount = (text) => Number(text.match(/^employees\[(\d+)\]/)?.[1])
  const rowCount = (text) => text.split('\n').length - 1

  assert.equal(declaredCount(control), 20)
  assert.equal(declaredCount(truncated), 20)
  assert.equal(declaredCount(extra), 20)
  assert.equal(rowCount(control), 20)
  assert.equal(rowCount(truncated), 17)
  assert.equal(rowCount(extra), 23)
  assert.equal(width.split('\n')[10].split(',').length, 5)

  for (const datasetId of [
    'structural-control',
    'structural-truncated',
    'structural-extra-rows',
    'structural-width-mismatch',
  ]) {
    assert.doesNotThrow(() => JSON.parse(find('json-compact', datasetId).text))
  }
})
