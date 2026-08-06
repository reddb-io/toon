#!/usr/bin/env node
import { mkdirSync, writeFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { encode } from '../../packages/toon/dist/index.js'
import {
  createBenchmarkSuite,
  encodeBenchmarkDocuments,
} from './suite.mjs'

const REPO_ROOT = dirname(dirname(dirname(fileURLToPath(import.meta.url))))
const RESULTS_DIR = join(REPO_ROOT, 'benchmarks', 'results')
const REPORT_PATH = join(RESULTS_DIR, 'retrieval-accuracy.md')
const provider = process.env.BENCHMARK_ACCURACY_PROVIDER ?? 'openai'
const model = process.env.BENCHMARK_ACCURACY_MODEL ?? 'gpt-4.1-mini'
const limit = parseLimit(process.env.BENCHMARK_ACCURACY_LIMIT)

if (provider !== 'openai') {
  console.error(`Unsupported BENCHMARK_ACCURACY_PROVIDER: ${provider}`)
  process.exit(2)
}
if (!process.env.OPENAI_API_KEY) {
  console.error('benchmark:accuracy needs OPENAI_API_KEY for provider=openai.')
  console.error('Export OPENAI_API_KEY, then rerun pnpm benchmark:accuracy.')
  process.exit(2)
}

const suite = createBenchmarkSuite()
const encoders = [
  { id: 'json-compact', format: 'json', encode: JSON.stringify },
  { id: 'toon-typescript', format: 'toon', encode },
  { id: 'toon-rust', format: 'toon', encode: encodeWithRust },
]
const documents = encodeBenchmarkDocuments(suite, encoders)
const selectedQuestions = limit === undefined ? suite.questions : suite.questions.slice(0, limit)
const results = []

for (const encoder of encoders) {
  for (const question of selectedQuestions) {
    const document = findDocument(documents, encoder.id, question.datasetId)
    const answer = await askOpenAI(encoder.id, document.text, question)
    const ok = validateAnswer(answer, question)
    results.push({
      encoderId: encoder.id,
      questionId: question.id,
      style: question.style,
      expected: displayValue(question.expected),
      answer,
      ok,
    })
    console.log(`${ok ? 'PASS' : 'FAIL'} ${encoder.id} ${question.id}: ${oneLine(answer)}`)
  }
}

mkdirSync(RESULTS_DIR, { recursive: true })
writeFileSync(REPORT_PATH, renderReport(suite, encoders, documents, results))
console.log(`accuracy ${results.filter((result) => result.ok).length}/${results.length} provider=${provider} model=${model}`)
console.log(`wrote ${REPORT_PATH}`)

function parseLimit(value) {
  if (value === undefined || value === '') return undefined
  const parsed = Number(value)
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    console.error('BENCHMARK_ACCURACY_LIMIT must be a positive integer.')
    process.exit(2)
  }
  return parsed
}

function encodeWithRust(value) {
  const result = spawnSync(
    'cargo',
    ['run', '--quiet', '-p', 'reddb-io-tq', '--', '-p', 'json', '-o', 'toon', '.'],
    {
      cwd: REPO_ROOT,
      input: JSON.stringify(value),
      encoding: 'utf8',
      maxBuffer: 10 * 1024 * 1024,
    },
  )
  if (result.error) throw new Error(`Rust encoder could not start: ${result.error.message}`)
  if (result.status !== 0) {
    throw new Error(`Rust encoder exited ${result.status}: ${result.stderr.trim()}`)
  }
  return result.stdout.trimEnd()
}

function findDocument(documents, encoderId, datasetId) {
  const document = documents.find((candidate) =>
    candidate.encoderId === encoderId && candidate.datasetId === datasetId)
  if (!document) throw new Error(`missing document for ${encoderId}/${datasetId}`)
  return document
}

async function askOpenAI(encoderId, document, question) {
  const response = await fetch('https://api.openai.com/v1/responses', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${process.env.OPENAI_API_KEY}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      model,
      input: [{
        role: 'user',
        content: [{
          type: 'input_text',
          text: [
            'Answer the question using only the encoded document.',
            'Return only the requested value, with no explanation.',
            `Encoder: ${encoderId}`,
            '',
            'Document:',
            document,
            '',
            `Question: ${question.prompt}`,
          ].join('\n'),
        }],
      }],
    }),
  })
  if (!response.ok) {
    throw new Error(`OpenAI request failed with HTTP ${response.status}`)
  }
  const body = await response.json()
  return body.output_text
    ?? body.output?.flatMap((item) => item.content ?? []).map((part) => part.text ?? '').join('')
    ?? ''
}

function validateAnswer(answer, question) {
  const value = unwrapAnswer(answer)
  if (question.answerType === 'integer' || question.answerType === 'number') {
    const match = String(value).replaceAll(',', '').match(/-?\d+(?:\.\d+)?/)
    if (!match) return false
    const actual = Number(match[0])
    return question.answerType === 'integer'
      ? actual === question.expected
      : Math.abs(actual - question.expected) <= 0.01
  }
  if (question.answerType === 'list') {
    const actual = Array.isArray(value)
      ? value
      : String(value).replace(/^fields?:\s*/i, '').split(',')
    return actual.map(normalizeText).join('|') === question.expected.map(normalizeText).join('|')
  }
  if (question.answerType === 'boolean') {
    const normalized = normalizeText(value)
    const expected = normalizeText(question.expected)
    return normalized === expected
      || (expected === 'yes' && normalized === 'true')
      || (expected === 'no' && normalized === 'false')
  }
  return normalizeText(value) === normalizeText(question.expected)
}

function unwrapAnswer(answer) {
  const text = String(answer ?? '')
    .trim()
    .replace(/^```(?:json)?\s*/i, '')
    .replace(/\s*```$/i, '')
    .trim()
  try {
    const parsed = JSON.parse(text)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      if ('answer' in parsed) return parsed.answer
      if ('value' in parsed) return parsed.value
      const values = Object.values(parsed)
      if (values.length === 1) return values[0]
    }
    return parsed
  } catch {
    return text
  }
}

function normalizeText(value) {
  return String(value).trim().replace(/^['"]|['"]$/g, '').toLowerCase()
}

function renderReport(suite, encoders, documents, results) {
  const lines = [
    '# Retrieval Accuracy Benchmark',
    '',
    'Command: `pnpm benchmark:accuracy`',
    '',
    `Suite: version ${suite.version}, seed ${suite.seed}`,
    `Provider: \`${provider}\``,
    `Model: \`${model}\``,
    `Questions per encoder: ${results.length / encoders.length}`,
    '',
    '## Accuracy by encoder',
    '',
    '| Encoder | Encoded bytes | Correct/total | Accuracy |',
    '| --- | ---: | ---: | ---: |',
  ]
  for (const encoder of encoders) {
    const encoderResults = results.filter((result) => result.encoderId === encoder.id)
    const correct = encoderResults.filter((result) => result.ok).length
    const bytes = documents
      .filter((document) => document.encoderId === encoder.id)
      .reduce((sum, document) => sum + document.bytes, 0)
    lines.push(`| ${encoder.id} | ${bytes} | ${correct}/${encoderResults.length} | ${percentage(correct, encoderResults.length)} |`)
  }

  lines.push('', '## Accuracy by question style', '')
  lines.push('| Encoder | Style | Correct/total | Accuracy |')
  lines.push('| --- | --- | ---: | ---: |')
  for (const encoder of encoders) {
    for (const style of ['structured-question', 'structural-corruption']) {
      const subset = results.filter((result) =>
        result.encoderId === encoder.id && result.style === style)
      const correct = subset.filter((result) => result.ok).length
      lines.push(`| ${encoder.id} | ${style} | ${correct}/${subset.length} | ${percentage(correct, subset.length)} |`)
    }
  }

  lines.push('', '## TypeScript/Rust output comparison', '')
  lines.push('| Scenario | TypeScript bytes | Rust bytes | Identical |')
  lines.push('| --- | ---: | ---: | --- |')
  for (const dataset of suite.datasets) {
    const typescript = findDocument(documents, 'toon-typescript', dataset.id)
    const rust = findDocument(documents, 'toon-rust', dataset.id)
    lines.push(`| ${dataset.id} | ${typescript.bytes} | ${rust.bytes} | ${typescript.text === rust.text ? 'yes' : 'no'} |`)
  }

  lines.push('', '## Detailed results', '')
  lines.push('| Encoder | Question | Expected | Answer | Result |')
  lines.push('| --- | --- | --- | --- | --- |')
  for (const result of results) {
    lines.push(`| ${result.encoderId} | ${result.questionId} | ${escapeCell(result.expected)} | ${escapeCell(result.answer)} | ${result.ok ? 'pass' : 'fail'} |`)
  }
  lines.push('')
  return `${lines.join('\n')}\n`
}

function percentage(correct, total) {
  return total === 0 ? 'n/a' : `${((correct / total) * 100).toFixed(1)}%`
}

function displayValue(value) {
  return Array.isArray(value) ? value.join(', ') : String(value)
}

function escapeCell(value) {
  return oneLine(value).replaceAll('|', '\\|')
}

function oneLine(value) {
  return String(value).replaceAll('\n', '<br>')
}
