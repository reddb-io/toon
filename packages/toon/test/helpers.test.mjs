import test from 'node:test'
import assert from 'node:assert/strict'

import {
  appendSummaryField,
  decode,
  encode,
  encodeToolManifest,
  projectFields,
} from '../dist/index.js'

test('appendSummaryField emits one conforming document with summary last', () => {
  const out = appendSummaryField({ service: 'checkout', rows: 3 }, { total: 3, failed: 1 })
  const back = decode(out)
  assert.deepEqual(back, { service: 'checkout', rows: 3, summary: { total: 3, failed: 1 } })
  const keys = Object.keys(back)
  assert.equal(keys[keys.length - 1], 'summary')
})

test('appendSummaryField replaces an existing summary key and moves it to the end', () => {
  const out = appendSummaryField({ summary: 'stale', a: 1 }, 'fresh')
  const back = decode(out)
  assert.deepEqual(back, { a: 1, summary: 'fresh' })
  assert.equal(Object.keys(back)[1], 'summary')
})

test('appendSummaryField output survives strings that need quoting', () => {
  const value = { note: 'a, b: [c] {d}\nnext' }
  const back = decode(appendSummaryField(value, 'ok'))
  assert.deepEqual(back, { note: 'a, b: [c] {d}\nnext', summary: 'ok' })
})

test('projectFields keeps allowlist order and drops other fields', () => {
  const rows = [
    { id: 1, state: 'active', noise: 'x' },
    { id: 2, state: 'merged', extra: true },
  ]
  const projected = projectFields(rows, ['state', 'id'])
  assert.deepEqual(projected, [
    { state: 'active', id: 1 },
    { state: 'merged', id: 2 },
  ])
  assert.deepEqual(Object.keys(projected[0]), ['state', 'id'])
})

test('projectFields leaves absent fields absent instead of null-filling', () => {
  const projected = projectFields([{ id: 1 }], ['id', 'missing'])
  assert.deepEqual(projected, [{ id: 1 }])
  assert.equal(Object.prototype.hasOwnProperty.call(projected[0], 'missing'), false)
})

test('encode/decode expose the authoritative v4.1 semantics', () => {
  assert.equal(encode({ value: 1 }), 'value: 1')
  assert.deepEqual(decode('# comment\nvalue: 1'), { value: 1 })
})

test('encodeToolManifest flattens MCP tool schemas into tabular params rows', () => {
  const tools = [
    {
      name: 'search_docs',
      description: 'Full-text search over the knowledge base',
      inputSchema: {
        type: 'object',
        properties: {
          query: { type: 'string', description: 'Search text' },
          limit: { type: 'integer', description: 'Max results' },
          tags: { type: 'array', items: { type: 'string' } },
          mode: { enum: ['fast', 'deep'] },
          cursor: { type: ['string', 'null'] },
        },
        required: ['query'],
      },
    },
    { name: 'ping' },
  ]

  const manifest = encodeToolManifest(tools)

  assert.equal(manifest, [
    'tools[2]:',
    '  - name: search_docs',
    '    description: Full-text search over the knowledge base',
    '    params[5]{name,type,required,description}:',
    '      query,string,true,Search text',
    '      limit,integer,false,Max results',
    '      tags,array<string>,false,""',
    '      mode,enum(fast|deep),false,""',
    '      cursor,string|null,false,""',
    '  - name: ping',
    '    description: ""',
    '    params: []',
  ].join('\n'))
  assert.deepEqual(decode(manifest).tools[0].params[0], {
    name: 'query',
    type: 'string',
    required: true,
    description: 'Search text',
  })
  // The prompt-facing manifest undercuts even minified JSON of the tool list.
  assert.ok(manifest.length < JSON.stringify(tools).length)
})
