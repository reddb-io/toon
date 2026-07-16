/**
 * Performance smoke guards, mirroring `tests/runners/rust/toon/perf_smoke.rs`.
 *
 * These are shape guards, not measurements: each bound is deliberately loose
 * enough that a slow or loaded CI machine never flakes, while the quadratic
 * behaviour they protect against — per-character string building, re-scanning
 * a row for every cell — blows past them by orders of magnitude.
 *
 * `html-payload.test.mjs` owns the specific multi-megabyte HTML case from the
 * GC-collapse regression (#194); these cover the other pathological shapes.
 * Detailed numbers belong in `pnpm benchmark:performance`, never in this gate.
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { encode, decode } from '../src/index.js'

const HTML_BLOCK = [
  '<section class="prose dark" data-config=\'{"key": [1, 2]}\'>',
  `  <script>if (a < b && c > d) { log("x, y: z", '\\\\srv\\\\share'); }</script>`,
  '  <p>Inline "quoted" text, commas: colons; <a href="https://example.com?a=1&b=2">link</a></p>',
  '</section>',
].join('\n')

function htmlString(targetBytes) {
  return HTML_BLOCK.repeat(Math.ceil(targetBytes / HTML_BLOCK.length))
}

/** Asserts a loose wall-clock ceiling, reporting the real cost on failure. */
function assertWithin(budgetMs, label, work) {
  const started = performance.now()
  work()
  const elapsed = performance.now() - started
  assert.ok(
    elapsed < budgetMs,
    `${label} took ${Math.round(elapsed)}ms, over the ${budgetMs}ms smoke budget — ` +
      'suspect a quadratic regression (measure with `pnpm benchmark:performance`)',
  )
}

test('one huge dense-quoted string round-trips', () => {
  const data = { body: htmlString(4_000_000) }
  assertWithin(60_000, '4MB single-string encode+decode', () => {
    assert.deepEqual(decode(encode(data)), data)
  })
})

test('a long tabular array encodes and decodes linearly', () => {
  const data = {
    rows: Array.from({ length: 20_000 }, (_, index) => ({
      id: index,
      name: `record-${index}`,
      note: 'text, with a comma and "quotes"',
      // Never integer-valued: SPEC §2 folds an integral float to its integer
      // form, which is a number-semantics concern the conformance corpus owns,
      // not this timing guard's business.
      score: index + 0.5,
      ok: index % 2 === 0,
    })),
  }
  assertWithin(60_000, '20k-row tabular encode+decode', () => {
    assert.deepEqual(decode(encode(data)), data)
  })
})

test('deeply nested objects encode and decode', () => {
  let data = { leaf: 'value, with delimiter' }
  for (let depth = 0; depth < 60; depth += 1) data = { child: data }
  assertWithin(30_000, '60-level nesting encode+decode', () => {
    assert.deepEqual(decode(encode(data)), data)
  })
})
