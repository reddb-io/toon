/**
 * HTML payloads inside JSON values — the strings that stress quoting the
 * hardest: dense `"` runs from attributes, `\` sequences, commas, colons and
 * brackets from inline scripts, and multi-line markup.
 *
 * The large-payload case also guards against the GC-thrashing regression
 * where per-character string building made multi-megabyte HTML strings take
 * minutes (or OOM) to encode/decode.
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { encode, decode } from '../src/index.js'

const HTML_SNIPPET = [
  '<!DOCTYPE html><html><head><title>Q&amp;A: "quotes", commas</title>',
  `<script>if (a < b && c > d) { console.log("x, y: z", '\\\\path\\\\file'); }</script>`,
  '</head><body class="main dark" data-config=\'{"key": [1, 2]}\'>',
  '<div id="app">Hello, "world"!\t<a href="https://example.com?a=1&b=2">link</a><br/></div>',
  '</body></html>',
].join('\n')

test('JSON rows carrying HTML round-trip exactly', () => {
  const data = {
    pages: Array.from({ length: 25 }, (_, index) => ({
      id: index,
      url: `https://example.com/page/${index}`,
      body: `${HTML_SNIPPET}<!-- page ${index} -->`,
    })),
  }
  assert.deepEqual(decode(encode(data)), data)
})

test('HTML in nested objects and mixed arrays round-trips exactly', () => {
  const data = {
    article: {
      title: 'On <em>markup</em>, "quoting" & escapes',
      blocks: [
        { type: 'html', content: HTML_SNIPPET },
        { type: 'text', content: 'plain' },
        '<hr class="divider"/>',
        42,
      ],
    },
  }
  assert.deepEqual(decode(encode(data)), data)
})

test('multi-megabyte HTML strings encode and decode without GC collapse', () => {
  const body = HTML_SNIPPET.repeat(Math.ceil(800_000 / HTML_SNIPPET.length))
  const data = {
    rows: Array.from({ length: 20 }, (_, index) => ({ id: index, body })),
  }

  const started = performance.now()
  const wire = encode(data)
  const roundTripped = decode(wire)
  const elapsed = performance.now() - started

  assert.deepEqual(roundTripped, data)
  // The per-character implementation took minutes / ran out of memory here;
  // the run-copying one takes ~1s. The bound is deliberately loose so slow CI
  // machines never flake, while a quadratic regression still fails.
  assert.ok(elapsed < 60_000, `encode+decode took ${Math.round(elapsed)}ms`)
})
