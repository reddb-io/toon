import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import {
  JsonlToToonl,
  ToonlDecodeStream,
  ToonlEncodeStream,
  ToonlToJsonl,
  ToonlCursorInvalidationError,
  ToonlEncoder,
  ToonlError,
  ToonlReader,
  closeTransform,
  decodeLines,
  encodeLines,
  encodeRecords,
  jsonToToon,
  parseRecords,
  parseStream,
  recordTransform,
  toonToJson,
} from '../src/index.js'
import { readToonlFile, writeToonlFile } from '../src/node.js'

async function collect(source) {
  const records = []
  for await (const record of decodeLines(source)) {
    records.push(record)
  }
  return records
}

async function readableText(readable) {
  let output = ''
  for await (const chunk of readable) {
    output += chunk
  }
  return output
}

function streamOf(chunks) {
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(chunk)
      }
      controller.close()
    },
  })
}

test('decodeLines yields one record per row, following schema rotation', async () => {
  const stream = '[]{id,name}:\n1,Ada\n[=1]\n[]{id,name,role}:\n2,Linus,dev\n[=1]\n'

  assert.deepEqual(await collect(stream), [
    { id: 1, name: 'Ada' },
    { id: 2, name: 'Linus', role: 'dev' },
  ])
})

test('decodeLines tolerates blank lines and a segment left open at EOF', async () => {
  assert.deepEqual(await collect('\n[|]{id|name}:\n1|Ada\n\n2|Linus\n'), [
    { id: 1, name: 'Ada' },
    { id: 2, name: 'Linus' },
  ])
})

test('decodeLines reads an async iterable whose chunks split mid-line', async () => {
  async function* chunks() {
    yield '[]{id,na'
    yield 'me}:\n1,Ada\n2,Li'
    yield 'nus\n[=2]\n'
  }

  assert.deepEqual(await collect(chunks()), [
    { id: 1, name: 'Ada' },
    { id: 2, name: 'Linus' },
  ])
})

test('decodeLines validates each segment trailer against the rows it saw', async () => {
  // `assert.rejects` resolves to nothing, so capture the error to inspect its line.
  let error
  try {
    await collect('[]{id,name}:\n1,Ada\n[=2]\n')
  } catch (rejection) {
    error = rejection
  }

  assert.ok(error instanceof ToonlError)
  assert.equal(error.line, 3)
  assert.equal(error.reason, 'trailer count mismatch')
})

test('decodeLines accepts matching continuation headers and rejects mismatches', async () => {
  assert.deepEqual(await collect('[]{id,name}:\n1,Ada\n[~]{id,name}:\n2,Linus\n[=2]\n'), [
    { id: 1, name: 'Ada' },
    { id: 2, name: 'Linus' },
  ])

  await assert.rejects(
    () => collect('[]{id,name}:\n1,Ada\n[~]{id,role}:\n2,dev\n'),
    /continuation header mismatch/,
  )
})

test('ToonlReader resumes from a serialized cursor', async () => {
  const stream = '[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n'
  const reader = new ToonlReader(stream)
  const iterator = reader[Symbol.asyncIterator]()

  assert.deepEqual((await iterator.next()).value, { id: 1, name: 'Ada' })
  const persisted = JSON.stringify(reader.cursor)
  const restored = JSON.parse(persisted)

  const resumed = []
  for await (const record of new ToonlReader(stream, { cursor: restored })) {
    resumed.push(record)
  }

  assert.deepEqual(resumed, (await collect(stream)).slice(1))
})

test('ToonlReader resumes across continuation headers', async () => {
  const stream = '[]{id,name}:\n1,Ada\n2,Linus\n[~]{id,name}:\n3,Grace\n[=3]\n'
  const reader = new ToonlReader(stream)
  const iterator = reader[Symbol.asyncIterator]()

  await iterator.next()
  await iterator.next()

  const resumed = []
  for await (const record of new ToonlReader(stream, { cursor: reader.cursor })) {
    resumed.push(record)
  }
  assert.deepEqual(resumed, [{ id: 3, name: 'Grace' }])
})

test('ToonlReader reports cursor invalidation distinctly', async () => {
  const stream = '[]{id,name}:\n1,Ada\n2,Linus\n'
  const reader = new ToonlReader(stream)
  const iterator = reader[Symbol.asyncIterator]()
  await iterator.next()

  await assert.rejects(
    () => collect(new ToonlReader(stream, { cursor: { byteOffset: 999, activeHeaderLine: '[]{id,name}:\n', rowsSinceHeader: 0 } })),
    (error) => error instanceof ToonlCursorInvalidationError && error.condition === 'truncated',
  )

  const rewritten = stream.replace('1,Ada', '9,Ada')
  await assert.rejects(
    () => collect(new ToonlReader(rewritten, { cursor: reader.cursor })),
    (error) => error instanceof ToonlCursorInvalidationError && error.condition === 'anchor-mismatch',
  )
})

test('decodeLines rejects rows before a header and reserved list prefixes', async () => {
  await assert.rejects(() => collect('1,Ada\n'), /row before header/)
  await assert.rejects(() => collect('[]{id,name}:\n- 1,Ada\n'), /reserved line prefix/)
  await assert.rejects(() => collect('[]{id,name}:\n1,Ada,extra\n'), /row arity mismatch/)
})

test('encodeLines writes the header lazily and rotates on a schema change', () => {
  const emitter = encodeLines()

  assert.equal(emitter.push({ id: 1, name: 'Ada' }), '[]{id,name}:\n1,Ada\n')
  assert.equal(emitter.push({ id: 2, name: 'Linus' }), '2,Linus\n')
  // A new field list closes the open segment and opens the next one.
  assert.equal(
    emitter.push({ id: 3, name: 'Grace', role: 'ops' }),
    '[=2]\n[]{id,name,role}:\n3,Grace,ops\n',
  )
  assert.equal(emitter.end(), '[=1]\n')
  assert.equal(emitter.end(), '')
})

test('encodeLines canonicalizes shuffled field order per record shape', () => {
  assert.equal(
    encodeRecords([
      { id: 1, name: 'Ada' },
      { name: 'Linus', id: 2 },
    ]),
    '[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n',
  )
})

test('encodeLines can leave the stream trailer-free', () => {
  assert.equal(
    encodeRecords([{ id: 1 }, { id: 2 }], { trailer: false }),
    '[]{id}:\n1\n2\n',
  )
  assert.equal(
    encodeRecords([{ id: 1 }, { id: 2 }], { delimiter: '|' }),
    '[|]{id}:\n1\n2\n[=2]\n',
  )
})

test('encodeLines emits continuation headers only when configured', () => {
  const records = [
    { id: 1, name: 'Ada' },
    { id: 2, name: 'Linus' },
    { id: 3, name: 'Grace' },
  ]

  assert.equal(encodeRecords(records), '[]{id,name}:\n1,Ada\n2,Linus\n3,Grace\n[=3]\n')
  assert.equal(
    encodeRecords(records, { continuationEveryRows: 2 }),
    '[]{id,name}:\n1,Ada\n2,Linus\n[~]{id,name}:\n3,Grace\n[=3]\n',
  )

  const encoder = new ToonlEncoder(',', ['id', 'name'])
  encoder.setContinuationEveryRows(2)
  encoder.pushRow({ id: 1, name: 'Ada' })
  encoder.pushRow({ id: 2, name: 'Linus' })
  encoder.pushRow({ id: 3, name: 'Grace' })
  assert.equal(encoder.finish(), '[]{id,name}:\n1,Ada\n2,Linus\n[~]{id,name}:\n3,Grace\n[=3]\n')
})

test('encodeLines output decodes back to the records it was given', async () => {
  const records = [
    { id: 1, name: 'Ada', active: true },
    { id: 2, name: 'Linus', active: false },
    { id: 3, name: 'Grace', active: true, role: 'ops' },
  ]

  assert.deepEqual(await collect(encodeRecords(records)), records)
})

test('encodeLines rejects rows TOONL cannot represent', () => {
  const emitter = encodeLines()

  assert.throws(() => emitter.push({ id: 1, tags: ['a'] }), /TOONL rows must be flat objects/)
  assert.throws(() => emitter.push({}), /TOONL output requires object rows/)
  assert.throws(() => emitter.push([1, 2]), /TOONL output requires object rows/)
})

test('ToonlEncoder validates its delimiter, fields and row arity', () => {
  assert.throws(() => new ToonlEncoder(';', ['id']), /invalid header delimiter/)
  assert.throws(() => new ToonlEncoder(',', []), /TOONL header requires fields/)

  const encoder = new ToonlEncoder(',', ['id', 'name'])
  assert.throws(() => encoder.pushRawRow(['1']), /row arity mismatch/)
  assert.throws(() => encoder.pushRow({ id: 1 }), /TOONL output schema changed/)

  encoder.pushRow({ id: 1, name: 'Ada' })
  assert.equal(encoder.rowCount, 1)
  assert.equal(encoder.finish(), '[]{id,name}:\n1,Ada\n[=1]\n')
})

test('parseStream keeps raw cells; parseRecords decodes them', () => {
  const stream = '[]{id,msg}:\n1,"disk full"\n[=1]\n'

  assert.deepEqual(parseStream(stream), [
    { delimiter: ',', fields: ['id', 'msg'], rows: [['1', '"disk full"']] },
  ])
  assert.deepEqual(parseRecords(stream), [{ id: 1, msg: 'disk full' }])
})

test('closeTransform emits one canonical TOON document per segment', () => {
  assert.deepEqual(closeTransform('[]{id,name}:\n1,Ada\n[=1]\n[|]{id|name}:\n2|Linus\n[=1]\n'), [
    '[1]{id,name}:\n  1,Ada\n',
    '[1|]{id|name}:\n  2|Linus\n',
  ])
})

test('Web Streams decode and encode TOONL records with schema rotation', async () => {
  const chunks = new ReadableStream({
    start(controller) {
      controller.enqueue('[]{id,na')
      controller.enqueue(new TextEncoder().encode('me}:\n1,Ada\n[=1]\n[]{id,name,role}:\n'))
      controller.enqueue('2,Linus,dev\n[=1]\n')
      controller.close()
    },
  })

  const encoded = chunks
    .pipeThrough(ToonlDecodeStream())
    .pipeThrough(
      recordTransform((record) => {
        if (record.id === 1) {
          return undefined
        }
        return { id: record.id, name: record.name, role: record.role }
      }),
    )

  assert.equal(await readableText(encoded), '[]{id,name,role}:\n2,Linus,dev\n[=1]\n')
})

test('JsonlToToonl and ToonlToJsonl bridge line streams in constant memory', async () => {
  const jsonl = ['{"id":1,"name":"Ada"}\n{"id":2,"name":"Linus","role":"dev"}\n']
  const toonl = await readableText(
    streamOf(jsonl).pipeThrough(JsonlToToonl()).pipeThrough(ToonlDecodeStream()).pipeThrough(ToonlEncodeStream()),
  )

  assert.equal(toonl, '[]{id,name}:\n1,Ada\n[=1]\n[]{id,name,role}:\n2,Linus,dev\n[=1]\n')
  assert.equal(
    await readableText(streamOf([toonl]).pipeThrough(ToonlToJsonl())),
    '{"id":1,"name":"Ada"}\n{"id":2,"name":"Linus","role":"dev"}\n',
  )
})

test('document bridge helpers convert whole JSON and TOON documents', () => {
  const toon = jsonToToon('{"users":[{"id":1,"name":"Ada"}]}')

  assert.equal(toon, 'users[1]{id,name}:\n  1,Ada\n')
  assert.equal(toonToJson(toon), '{"users":[{"id":1,"name":"Ada"}]}')
})

test('node subpath reads and writes TOONL files as record streams', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'toon-node-'))
  const path = join(directory, 'records.toonl')

  try {
    await writeToonlFile(path, [
      { id: 1, name: 'Ada' },
      { id: 2, name: 'Linus', role: 'dev' },
    ])

    assert.equal(
      readFileSync(path, 'utf8'),
      '[]{id,name}:\n1,Ada\n[=1]\n[]{id,name,role}:\n2,Linus,dev\n[=1]\n',
    )

    const records = []
    for await (const record of readToonlFile(path)) {
      records.push(record)
    }
    assert.deepEqual(records, [
      { id: 1, name: 'Ada' },
      { id: 2, name: 'Linus', role: 'dev' },
    ])
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
