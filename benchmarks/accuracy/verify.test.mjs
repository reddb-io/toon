import assert from 'node:assert/strict'
import test from 'node:test'

import { encode } from '../../packages/toon/dist/index.js'
import {
  createBenchmarkSuite,
  encodeBenchmarkDocuments,
} from './suite.mjs'

test('dataset and question generation is deterministic', () => {
  const first = createBenchmarkSuite()
  const second = createBenchmarkSuite()

  assert.deepEqual(first, second)
  assert.equal(first.seed, 218)
  assert.equal(first.datasets.length, 5)
  assert.equal(first.questions.length, 13)
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
