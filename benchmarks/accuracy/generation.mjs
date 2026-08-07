import { isDeepStrictEqual } from 'node:util'

const REQUIRED_OBSERVATION_FIELDS = [
  'taskId',
  'model',
  'prompt',
  'promptBudget',
  'format',
  'retries',
  'tokens',
  'syntaxValid',
  'semanticAccuracy',
  'provenance',
  'rawArtifactRefs',
]

export function evaluateGenerationAttempts({ task, format, attempts, decode }) {
  if (!task || typeof task !== 'object') throw new TypeError('generation task is required')
  if (!['json', 'toon'].includes(format)) throw new TypeError(`unsupported generation format: ${format}`)
  if (!Array.isArray(attempts) || attempts.length === 0) {
    throw new TypeError('generation attempts must be a non-empty array')
  }
  if (typeof decode !== 'function') throw new TypeError(`decoder is required for ${format}`)

  const evaluated = []
  for (const attempt of attempts) {
    const result = validateGenerationOutput(attempt.raw, task.expected, decode)
    evaluated.push({
      rawArtifactRef: attempt.rawArtifactRef,
      raw: attempt.raw,
      ...result,
    })
    if (result.semanticValid) break
  }
  const final = evaluated.at(-1)

  return {
    taskId: task.id,
    format,
    promptBudget: task.promptBudget,
    retries: evaluated.length - 1,
    syntaxValid: final.syntaxValid,
    semanticValid: final.semanticValid,
    attempts: evaluated,
  }
}

export function validateGenerationOutput(raw, expected, decode) {
  try {
    const value = decode(String(raw))
    return {
      syntaxValid: true,
      semanticValid: isDeepStrictEqual(value, expected),
    }
  } catch (error) {
    return {
      syntaxValid: false,
      semanticValid: false,
      syntaxError: error instanceof Error ? error.message : String(error),
    }
  }
}

export function validateAccuracyReport(report) {
  requireObject(report, 'report')
  requireInteger(report.schemaVersion, 'schemaVersion', 1)
  requireObject(report.verification, 'verification')
  if (report.verification.mode !== 'offline') throw new TypeError('verification.mode must be offline')
  if (report.verification.reproducible !== true) {
    throw new TypeError('verification.reproducible must be true')
  }
  requireString(report.verification.command, 'verification.command')
  requireInteger(report.verification.suiteVersion, 'verification.suiteVersion', 1)
  requireInteger(report.verification.seed, 'verification.seed', 0)
  if (!Array.isArray(report.observations) || report.observations.length === 0) {
    throw new TypeError('observations must be a non-empty array')
  }

  for (const [index, observation] of report.observations.entries()) {
    const prefix = `observations[${index}]`
    requireObject(observation, prefix)
    for (const field of REQUIRED_OBSERVATION_FIELDS) {
      if (!Object.hasOwn(observation, field)) throw new TypeError(`${prefix}.${field} is required`)
    }
    requireString(observation.taskId, `${prefix}.taskId`)
    requireString(observation.model, `${prefix}.model`)
    requireString(observation.prompt, `${prefix}.prompt`)
    requireInteger(observation.promptBudget, `${prefix}.promptBudget`, 1)
    if (!['json', 'toon'].includes(observation.format)) {
      throw new TypeError(`${prefix}.format must be json or toon`)
    }
    requireInteger(observation.retries, `${prefix}.retries`, 0)
    requireTokens(observation.tokens, `${prefix}.tokens`)
    if (typeof observation.syntaxValid !== 'boolean') {
      throw new TypeError(`${prefix}.syntaxValid must be boolean`)
    }
    if (typeof observation.semanticAccuracy !== 'number'
      || observation.semanticAccuracy < 0
      || observation.semanticAccuracy > 1) {
      throw new TypeError(`${prefix}.semanticAccuracy must be between 0 and 1`)
    }
    requireObject(observation.provenance, `${prefix}.provenance`)
    if (observation.provenance.kind !== 'non-ci-model-observation') {
      throw new TypeError(`${prefix}.provenance.kind must be non-ci-model-observation`)
    }
    requireString(observation.provenance.provider, `${prefix}.provenance.provider`)
    requireString(observation.provenance.observedAt, `${prefix}.provenance.observedAt`)
    if (!Array.isArray(observation.rawArtifactRefs) || observation.rawArtifactRefs.length === 0) {
      throw new TypeError(`${prefix}.rawArtifactRefs must be a non-empty array`)
    }
    observation.rawArtifactRefs.forEach((reference, referenceIndex) =>
      requireString(reference, `${prefix}.rawArtifactRefs[${referenceIndex}]`))
  }

  assertComparableObservations(report.observations)
  return report
}

export function renderAccuracyReportSnapshot(report) {
  validateAccuracyReport(report)
  const lines = [
    '# Accuracy Benchmark Report',
    '',
    '## Offline verification (reproducible)',
    '',
    `- Command: \`${report.verification.command}\``,
    `- Suite: version ${report.verification.suiteVersion}, seed ${report.verification.seed}`,
    '- LLM access: not required',
    '',
    '## Model observations (non-CI)',
    '',
    'These model observations are reporting artifacts and are not merge-gate evidence.',
    '',
    '| Task | Model | Format | Retries | Tokens | Syntax valid | Semantic accuracy | Raw artifacts |',
    '| --- | --- | --- | ---: | ---: | --- | ---: | --- |',
  ]
  for (const observation of report.observations) {
    lines.push([
      observation.taskId,
      observation.model,
      observation.format,
      observation.retries,
      observation.tokens.total,
      observation.syntaxValid ? 'yes' : 'no',
      `${(observation.semanticAccuracy * 100).toFixed(1)}%`,
      observation.rawArtifactRefs.join('<br>'),
    ].map(escapeCell).join(' | ').replace(/^/, '| ').replace(/$/, ' |'))
  }
  lines.push('')
  return `${lines.join('\n')}\n`
}

function assertComparableObservations(observations) {
  const groups = new Map()
  for (const observation of observations) {
    const key = observation.encoderId ?? observation.format
    const group = groups.get(key) ?? { format: observation.format, tasks: [] }
    group.tasks.push({ taskId: observation.taskId, promptBudget: observation.promptBudget })
    groups.set(key, group)
  }
  const json = [...groups.values()].find((group) => group.format === 'json')
  const toon = [...groups.values()].filter((group) => group.format === 'toon')
  if (!json || toon.length === 0
    || toon.some((group) => !isDeepStrictEqual(json.tasks, group.tasks))) {
    throw new TypeError('JSON and TOON observations must use identical tasks and prompt budgets')
  }
}

function requireTokens(tokens, path) {
  requireObject(tokens, path)
  requireInteger(tokens.input, `${path}.input`, 0)
  requireInteger(tokens.output, `${path}.output`, 0)
  requireInteger(tokens.total, `${path}.total`, 0)
  if (tokens.total !== tokens.input + tokens.output) {
    throw new TypeError(`${path}.total must equal input + output`)
  }
}

function requireObject(value, path) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new TypeError(`${path} must be an object`)
  }
}

function requireString(value, path) {
  if (typeof value !== 'string' || value.length === 0) throw new TypeError(`${path} must be a string`)
}

function requireInteger(value, path, minimum) {
  if (!Number.isSafeInteger(value) || value < minimum) {
    throw new TypeError(`${path} must be an integer >= ${minimum}`)
  }
}

function escapeCell(value) {
  return String(value).replaceAll('|', '\\|').replaceAll('\n', '<br>')
}
