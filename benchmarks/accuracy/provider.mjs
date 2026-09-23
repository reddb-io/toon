/**
 * Provider plumbing for the accuracy harness, kept free of network and
 * process side effects so `verify.test.mjs` can pin it offline: the endpoint
 * (any OpenAI-compatible gateway via OPENAI_BASE_URL), the Responses request
 * shape (including the JSON-object-mode baseline), and the run provenance
 * written next to each report.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const DEFAULT_BASE_URL = 'https://api.openai.com/v1'

export function resolveProvider(env) {
  return {
    provider: env.BENCHMARK_ACCURACY_PROVIDER ?? 'openai',
    model: env.BENCHMARK_ACCURACY_MODEL ?? 'gpt-4.1-mini',
    baseUrl: (env.OPENAI_BASE_URL || DEFAULT_BASE_URL).replace(/\/+$/, ''),
    dryRun: ['1', 'true'].includes(env.BENCHMARK_ACCURACY_DRY_RUN ?? ''),
  }
}

/** One Responses API request; JSON-object mode is the structured-output baseline. */
export function buildResponsesRequest({ model, prompt, maxOutputTokens, jsonObjectMode = false }) {
  return {
    model,
    input: [{
      role: 'user',
      content: [{ type: 'input_text', text: prompt }],
    }],
    ...(maxOutputTokens === undefined ? {} : { max_output_tokens: maxOutputTokens }),
    ...(jsonObjectMode ? { text: { format: { type: 'json_object' } } } : {}),
  }
}

/** Upper bound of model calls a run makes: one per retrieval question, up to three per generation task. */
export function plannedRequests({ encoders, questions, generationTasks, maxAttempts = 3 }) {
  const retrieval = encoders.filter((encoder) => !encoder.generationOnly).length * questions
  const generation = encoders.length * generationTasks * maxAttempts
  return { retrieval, generation, total: retrieval + generation }
}

/**
 * What a reader needs to reproduce or distrust a run: the code revision, the
 * runtime, the endpoint host (never a credential), the settings, and a hash of
 * every source file that shaped the prompts and the scoring.
 */
export function runMetadata({ repoRoot, gitRevision, observedAt, settings, suite, sources }) {
  return {
    schemaVersion: 1,
    observedAt,
    gitRevision,
    node: process.version,
    provider: settings.provider,
    model: settings.model,
    endpointHost: new URL(settings.baseUrl).host,
    limit: settings.limit ?? null,
    suiteVersion: suite.version,
    seed: suite.seed,
    sourceHashes: Object.fromEntries(sources.map((source) => [
      source,
      createHash('sha256').update(readFileSync(join(repoRoot, source))).digest('hex'),
    ])),
  }
}
