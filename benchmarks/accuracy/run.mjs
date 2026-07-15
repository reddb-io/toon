#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { encodeRecords, serialize } from '../../packages/toon/src/index.js'

const REPO_ROOT = dirname(dirname(dirname(fileURLToPath(import.meta.url))))
const RESULTS_DIR = join(REPO_ROOT, 'benchmarks', 'results')
const REPORT_PATH = join(RESULTS_DIR, '2026-07-15-retrieval-accuracy.md')

const provider = process.env.BENCHMARK_ACCURACY_PROVIDER ?? 'openai'
const model = process.env.BENCHMARK_ACCURACY_MODEL ?? 'gpt-4.1-mini'

if (provider === 'openai' && !process.env.OPENAI_API_KEY) {
  console.error('benchmark:accuracy needs OPENAI_API_KEY for provider=openai.')
  console.error('Create a local .env from .env.example or export OPENAI_API_KEY, then rerun pnpm benchmark:accuracy.')
  process.exit(2)
}

const fixture = JSON.parse(readFileSync(join(REPO_ROOT, 'tests/corpus/wire-efficiency/corpora.json'), 'utf8'))
const sample = fixture.cases.find((testCase) => testCase.name === 'shipments-500').value.shipments.slice(0, 25)
const formats = {
  json: JSON.stringify({ shipments: sample }),
  toon: serialize({ shipments: sample }),
  toonExt: serialize({ shipments: sample }, { primitiveArrayColumns: true, objectArrayColumns: true, nestedTabularHeaders: true }),
  toonl: encodeRecords(sample),
}

const questions = [
  {
    id: 'count-express',
    prompt: 'How many shipments have priority equal to express?',
    expected: sample.filter((row) => row.priority === 'express').length,
    type: 'number',
  },
  {
    id: 'first-destination',
    prompt: 'What is the destination of shipment shp_00001?',
    expected: sample.find((row) => row.id === 'shp_00001').destination,
    type: 'string',
  },
]

function validate(answer, question) {
  const normalized = normalizeAnswer(answer, question)
  if (question.type === 'number') {
    return Number(normalized) === question.expected
  }
  return String(normalized).trim() === String(question.expected)
}

function normalizeAnswer(answer, question) {
  let text = String(answer ?? '').trim()
  text = text.replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/i, '').trim()
  try {
    const parsed = JSON.parse(text)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      const values = Object.values(parsed)
      if (values.length === 1) return values[0]
      if ('answer' in parsed) return parsed.answer
      if ('value' in parsed) return parsed.value
      if ('destination' in parsed) return parsed.destination
    }
    return parsed
  } catch {
    // Plain text answers are expected; JSON parsing only handles wrappers.
  }

  if (question.type === 'number') {
    const match = text.match(/-?\d+(?:\.\d+)?/)
    return match ? match[0] : text
  }

  const expected = String(question.expected)
  if (text.includes(':')) {
    text = text.slice(text.lastIndexOf(':') + 1).trim()
  }
  const lines = text.split(/\r?\n/).map((line) => line.trim()).filter(Boolean)
  if (lines.length > 0) {
    text = lines[lines.length - 1]
  }
  return text.includes(expected) ? expected : text
}

async function askOpenAI(format, question) {
  const response = await fetch('https://api.openai.com/v1/responses', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${process.env.OPENAI_API_KEY}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      model,
      input: [
        {
          role: 'user',
          content: [
            { type: 'input_text', text: `Answer with only the value.\n\nFormat: ${format}\n\nData:\n${formats[format]}\n\nQuestion: ${question.prompt}` },
          ],
        },
      ],
    }),
  })
  if (!response.ok) {
    throw new Error(`OpenAI request failed: ${response.status} ${await response.text()}`)
  }
  const body = await response.json()
  return body.output_text ?? body.output?.flatMap((item) => item.content ?? []).map((part) => part.text ?? '').join('')
}

let passed = 0
let total = 0
const rows = []
for (const format of Object.keys(formats)) {
  for (const question of questions) {
    total += 1
    const answer = await askOpenAI(format, question)
    const ok = validate(answer, question)
    passed += ok ? 1 : 0
    rows.push({ format, question: question.id, expected: question.expected, answer, ok })
    console.log(`${ok ? 'PASS' : 'FAIL'} ${format} ${question.id}: ${answer}`)
  }
}

console.log(`accuracy ${passed}/${total} provider=${provider} model=${model}`)
mkdirSync(RESULTS_DIR, { recursive: true })
writeFileSync(REPORT_PATH, renderReport(rows, passed, total))
console.log(`wrote ${REPORT_PATH}`)

function renderReport(results, passedCount, totalCount) {
  const lines = [
    '# Retrieval Accuracy Benchmark',
    '',
    'Command: `pnpm benchmark:accuracy`',
    '',
    `Provider: \`${provider}\``,
    `Model: \`${model}\``,
    `Score: ${passedCount}/${totalCount}`,
    '',
    '| Format | Question | Expected | Answer | Result |',
    '| --- | --- | --- | --- | --- |',
  ]
  for (const row of results) {
    lines.push(`| ${row.format} | ${row.question} | ${escapeCell(row.expected)} | ${escapeCell(row.answer)} | ${row.ok ? 'pass' : 'fail'} |`)
  }
  lines.push('')
  return `${lines.join('\n')}\n`
}

function escapeCell(value) {
  return String(value).replaceAll('|', '\\|').replaceAll('\n', '<br>')
}
