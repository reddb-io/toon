#!/usr/bin/env node
/**
 * Generates the `html-heavy` performance dataset: JSON records whose string
 * values are dense markup. This is the shape that stresses the quoting path
 * hardest — long runs of `"`, `\`, commas, colons and brackets inside one
 * string — and it is the real payload class behind the GC-thrashing encode
 * regression fixed in #194.
 *
 * The dataset belongs to the performance axis, not to the token corpus in
 * `benchmarks/datasets/`: it is chosen for a pathological *cost* profile, not
 * as a representative payload shape, and the token report's shape taxonomy is
 * deliberately anti-cherry-pick.
 */
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)))
const OUTPUT_DIR = join(REPO_ROOT, 'benchmarks/performance/datasets/html-heavy')
const SEED = 0x5eed_0198

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

function pick(random, values) {
  return values[Math.floor(random() * values.length)]
}

const SECTIONS = ['docs', 'blog', 'guides', 'reference', 'changelog']
const CLASSES = ['prose dark', 'main narrow', 'layout grid', 'card shadow-sm']
const TITLES = [
  'Q&amp;A: "quotes", commas &amp; escapes',
  'Escaping <em>markup</em> in "strings"',
  'Why "\\" and "," fight the encoder',
  'Attributes, delimiters: a "field" guide',
]

/**
 * One block of markup with every character class the quoting path branches on:
 * attribute quotes, backslash runs, commas, colons, brackets and newlines.
 */
function block(random, index) {
  const cls = pick(random, CLASSES)
  const title = pick(random, TITLES)
  return [
    `<section class="${cls}" data-index="${index}" data-config='{"key": [1, 2], "path": "C:\\\\tmp\\\\f.txt"}'>`,
    `  <h2 id="h-${index}">${title}</h2>`,
    `  <script>if (a < b && c > d) { log("x, y: z", '\\\\srv\\\\share'); }</script>`,
    `  <p>Inline "quoted" text, with commas: colons; and <a href="https://example.com/p?a=1&b=2">a link</a>.</p>`,
    `  <pre><code>{"nested": "json", "in": ["a", "b"], "esc": "\\"done\\""}</code></pre>`,
    '</section>',
  ].join('\n')
}

function page(random, index, blocksPerPage) {
  const body = [
    '<!DOCTYPE html><html><head>',
    `<title>${pick(random, TITLES)}</title>`,
    '<meta name="description" content="A page whose &quot;body&quot; is markup, not prose.">',
    `</head><body class="${pick(random, CLASSES)}">`,
    ...Array.from({ length: blocksPerPage }, (_, blockIndex) => block(random, blockIndex)),
    '</body></html>',
  ].join('\n')

  return {
    id: `page_${String(index + 1).padStart(4, '0')}`,
    url: `https://example.com/${pick(random, SECTIONS)}/page-${index + 1}`,
    title: pick(random, TITLES),
    section: pick(random, SECTIONS),
    renderedAt: `2026-07-${String((index % 28) + 1).padStart(2, '0')}T09:00:00Z`,
    body,
  }
}

function dataset(pageCount, blocksPerPage) {
  const random = mulberry32(SEED)
  return { pages: Array.from({ length: pageCount }, (_, index) => page(random, index, blocksPerPage)) }
}

function write(name, value) {
  const path = join(OUTPUT_DIR, name)
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`)
  console.log(`${name}: ${value.pages.length} pages, ${JSON.stringify(value).length} JSON bytes`)
}

mkdirSync(OUTPUT_DIR, { recursive: true })
write('rendered-pages-small.json', dataset(6, 2))
write('rendered-pages-large.json', dataset(60, 6))
