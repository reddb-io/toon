import assert from 'node:assert/strict'
import test from 'node:test'

import { comparisonReport } from './run.mjs'

test('comparison report puts every implementation side by side for encode and decode', () => {
  const markdown = comparisonReport(
    [
      {
        name: 'benchmarks/datasets/example-small',
        operations: {
          encode: {
            inputBytes: 1024 * 1024,
            measurements: {
              oursTs: 100,
              oursRustTq: 200,
              upstream: 400,
            },
          },
          decode: {
            inputBytes: 512 * 1024,
            measurements: {
              oursTs: 50,
              oursRustTq: 100,
              upstream: 200,
            },
          },
        },
      },
    ],
    { nodeVersion: 'v-test', upstreamRevision: 'abcdef0', minSamples: 5, minTotalMs: 400 },
  )

  assert.match(markdown, /upstream: `vendor\/toon` at `abcdef0`/)
  assert.match(
    markdown,
    /\| Dataset \| Input bytes \| Ours TS MiB\/s \| Ours TS ops\/s \| Ours Rust \(`tq`\) MiB\/s \| Ours Rust \(`tq`\) ops\/s \| Upstream MiB\/s \| Upstream ops\/s \|/,
  )
  assert.match(
    markdown,
    /\| `benchmarks\/datasets\/example-small` \| 1048576 \| 10\.0 \| 10 \| 5\.0 \| 5 \| 2\.5 \| 3 \|/,
  )
  assert.match(markdown, /## Encode[\s\S]*## Decode/)
})
