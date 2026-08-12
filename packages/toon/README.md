# @reddb-io/toon

> **Attribution:** This is RedDB's TypeScript implementation of TOON - not the original project. The TOON format was created by Johann Schopplich; see the [official repo](https://github.com/toon-format/toon), [toon-format/spec](https://github.com/toon-format/spec), and [toonformat.dev](https://toonformat.dev) for the format spec and original project.

TOON v4.1 parser and serializer, plus TOONL v0.2 append-only streaming, in dependency-free ESM.

TOON ([Token-Oriented Object Notation](https://github.com/toon-format/spec)) is JSON's data model in a compact model-facing layout. This package decodes TOON to plain JSON values and encodes them back to canonical TOON. It also implements the reddb-io opt-in extensions specified in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md) and the TOONL streaming layer specified in [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md).

The runtime has zero dependencies. TypeScript sources are compiled into the
published `dist/` JavaScript and declarations; the release workflow verifies
that build before publishing. Performance notes and token-efficiency
measurements live in [`benchmarks/`](../../benchmarks/README.md), not in this
package README.

```bash
pnpm add @reddb-io/toon
```

## TOON

```js
import { decode, encode } from '@reddb-io/toon'

const document = decode('users[2]{id,name}:\n  1,Ada\n  2,Linus\n')
console.log(JSON.stringify(document))

process.stdout.write(`${encode(document)}\n`)
console.log('round-trip', JSON.stringify(decode(encode(document))) === JSON.stringify(document))
```
```console
{"users":[{"id":1,"name":"Ada"},{"id":2,"name":"Linus"}]}
```
```console
users[2]{id,name}:
  1,Ada
  2,Linus
```
```console
round-trip true
```

- `decode(input, options?)` decodes a TOON document to a JSON value. `decodeFromLines(lines, options?)` accepts pre-split lines.
- `decodeStream` and `decodeStreamSync` expose positioned JSON-semantic events without building a value tree.
- `encode(value, options?)` encodes canonical TOON. `encodeLines(value, options?)` yields its lines without trailing newlines.
- `DELIMITERS`, `DEFAULT_DELIMITER`, `rawString`, `escapeString`, and `ToonDecodeError` match the canonical v4.1 helper surface.
- `detectTruncation(input, { format?: 'toon' | 'toonl', ...parseOptions })` returns a structured completeness report instead of throwing. Complete input reports `complete: true`; truncated TOON arrays, cut nested bodies, TOONL trailer mismatches, and missing TOONL trailers report `kind`, `line`, `declared`, `actual`, and `message`.

Pre-v4 dotted-key expansion and the former permissive codec live only at the explicit `@reddb-io/toon/legacy` subpath. New code should not import it.

Strict mode is on by default. It enforces the official TOON error checklist; pass `{ strict: false }` only when you intentionally want legacy recovery behavior.

### Experimental Decode Reviver

`decode` and `decodeFromLines` accept an experimental `reviver` option. This
frontier is audited from upstream PR
[`toon-format/toon#294`](https://github.com/toon-format/toon/pull/294) at commit
`b3e4f61609ee7d676c0066440964a9f01ab767b7`; it is not normative TOON v4.1
behavior and may change when that PR merges or ships in an upstream release.
Calls are depth-first and bottom-up. The callback receives a string property
key, the current value, and a root-relative path whose array segments are
numbers. Returning `undefined` deletes object properties and compacts array
elements; for the root value it means no change. Other replacements are
normalized to the JSON data model. Callback errors propagate unchanged.

```js
import { decode } from '@reddb-io/toon'

const document = decode('user:\n  name: Ada\n  password: secret\nroles[2]: admin,reader', {
  reviver(key, value, path) {
    if (key === 'password') return undefined
    if (path[0] === 'roles' && typeof value === 'string') return value.toUpperCase()
    return value
  },
})

console.log(JSON.stringify(document))
```
```console
{"user":{"name":"Ada"},"roles":["ADMIN","READER"]}
```

Omitting `reviver` leaves ordinary decode unchanged. `decodeStream` and
`decodeStreamSync` remain the normative event APIs and do not run this hook.

### Base Options

- `indent` changes how the parser interprets leading spaces. Serialization stays canonical TOON v4.1 with two-space indentation by default.

```js
import { decode, encode } from '@reddb-io/toon'

const input = 'person:\n    city: London\n'

try {
  decode(input)
} catch (error) {
  console.log('default indent', error.message)
}

const document = decode(input, { indent: 4 })
console.log('indent 4', JSON.stringify(document))
process.stdout.write(`${encode(document)}\n`)
console.log('round-trip', JSON.stringify(decode(encode(document))) === JSON.stringify(document))
```
```console
default indent Line 2: over-indented line
```
```console
indent 4 {"person":{"city":"London"}}
```
```console
person:
  city: London
```
```console
round-trip true
```

- `strict` is on by default. Turning it off keeps legacy last-write-wins recovery for duplicate keys.

```js
import { decode } from '@reddb-io/toon'

const input = 'a: 1\na: 2\n'

try {
  decode(input)
} catch (error) {
  console.log('strict default', error.message)
}

console.log('strict false', JSON.stringify(decode(input, { strict: false })))
```
```console
strict default Line 2: duplicate object key
```
```console
strict false {"a":2}
```

- `maxDepth` guards both decode and encode recursion. Set `0` only when the input is trusted and you intentionally want to disable the depth guard.

```js
import { decode, encode } from '@reddb-io/toon'

const input = 'a:\n  b:\n    c: 1\n'

try {
  decode(input, { maxDepth: 1 })
} catch (error) {
  console.log('maxDepth 1', error.message)
}

const document = decode(input, { maxDepth: 0 })
console.log('maxDepth 0', JSON.stringify(document))
process.stdout.write(`${encode(document, { maxDepth: 0 })}\n`)
console.log('round-trip', JSON.stringify(decode(encode(document))) === JSON.stringify(document))
```
```console
maxDepth 1 Line 3: maximum nesting depth exceeded (maxDepth 1)
```
```console
maxDepth 0 {"a":{"b":{"c":1}}}
```
```console
a:
  b:
    c: 1
```
```console
round-trip true
```

### Encode Extensions

With no encode options, output remains canonical TOON v4.1. Nested field groups
and keyed tabular maps are not extensions: v4.1 absorbed them and the canonical
encoder uses them automatically. Primitive-array and object-array extension
wires are recognized by default and emitted only when requested. Cyclic
discriminated arrays require an explicit option for both reconstruction and
emission because their wire is also valid as a literal v4.1 object. The
extension model is specified in
[`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md).

In every example below, the `assert` lines are the guarantees — lossless round-trip, and canonical fallback for ineligible data — kept executable without polluting the output, which is always a plain TOON document.

- `primitiveArrayColumns` emits primitive list columns such as `tags[;]` inside otherwise tabular object arrays. Spec: [Primitive-array columns](../../docs/proposals/primitive-array-columns.md).
  By default, or when a row is not eligible, output falls back losslessly to canonical TOON v4.1.

Default output, canonical v4.1:

```js
import { encode } from '@reddb-io/toon'

const value = { users: [{ id: 1, tags: ['red', 'blue'] }] }
process.stdout.write(`${encode(value)}\n`)
```
```console
users[1]:
  - id: 1
    tags[2]: red,blue
```

The same value with `primitiveArrayColumns: true`:

```js
import assert from 'node:assert/strict'
import { decode, encode } from '@reddb-io/toon'

const value = { users: [{ id: 1, tags: ['red', 'blue'] }] }
const enabled = encode(value, { primitiveArrayColumns: true })
process.stdout.write(`${enabled}\n`)
assert.deepEqual(decode(enabled), value)

const ineligible = { users: [{ id: 1, tags: null }, { id: 2, tags: ['ok'] }] }
assert.equal(encode(ineligible, { primitiveArrayColumns: true }), encode(ineligible))
```
```console
users[1]{id,tags[;]}:
  1,red;blue
```

- `objectArrayColumns` emits child tables for array-valued object columns. Spec: [Child tables and matrix](../../docs/proposals/child-tables-and-matrix.md).
  By default, or when a child array is not eligible, output falls back losslessly to canonical TOON v4.1.

Default output, canonical v4.1:

```js
import { encode } from '@reddb-io/toon'

const value = { orders: [{ id: 1, items: [{ sku: 'A', qty: 2 }, { sku: 'B', qty: 1 }] }] }
process.stdout.write(`${encode(value)}\n`)
```
```console
orders[1]:
  - id: 1
    items[2]{sku,qty}:
      A,2
      B,1
```

The same value with `objectArrayColumns: true`:

```js
import assert from 'node:assert/strict'
import { decode, encode } from '@reddb-io/toon'

const value = { orders: [{ id: 1, items: [{ sku: 'A', qty: 2 }, { sku: 'B', qty: 1 }] }] }
const enabled = encode(value, { objectArrayColumns: true })
process.stdout.write(`${enabled}\n`)
assert.deepEqual(decode(enabled), value)

const ineligible = { orders: [{ id: 1, items: [{ sku: 'A' }] }, { id: 2, items: [1] }] }
assert.equal(encode(ineligible, { objectArrayColumns: true }), encode(ineligible))
```
```console
orders[1]{id,items{sku,qty}}:
  1,2
    A,2
    B,1
```

- `cyclicDiscriminatedArrays` emits the specialized wire for eligible top-level event arrays whose discriminator values repeat in a stable cycle. Spec: [Cyclic discriminated arrays](../../docs/proposals/cyclic-discriminated-arrays.md).
  By default, or when the discriminator order is not eligible, output falls back losslessly to canonical TOON v4.1.

Default output, canonical v4.1 — the discriminator repeats in every row:

```js
import { encode } from '@reddb-io/toon'

const value = { events: [] }
for (let index = 1; index <= 12; index += 1) {
  const type = ['login', 'purchase', 'logout'][(index - 1) % 3]
  value.events.push({ type, payload: { id: `evt_${index}` } })
}
process.stdout.write(`${encode(value)}\n`)
```
```console
events[12]{type,payload{id}}:
  login,evt_1
  purchase,evt_2
  logout,evt_3
  login,evt_4
  purchase,evt_5
  logout,evt_6
  login,evt_7
  purchase,evt_8
  logout,evt_9
  login,evt_10
  purchase,evt_11
  logout,evt_12
```

The same value with `cyclicDiscriminatedArrays: true` — the `order`, `discriminator`, and `rows` fields are data (a strict TOON v4.1 decoder reads them as a literal object), not mode flags:

```js
import assert from 'node:assert/strict'
import { decode, encode } from '@reddb-io/toon'

const value = { events: [] }
for (let index = 1; index <= 12; index += 1) {
  const type = ['login', 'purchase', 'logout'][(index - 1) % 3]
  value.events.push({ type, payload: { id: `evt_${index}` } })
}
const enabled = encode(value, { cyclicDiscriminatedArrays: true })
process.stdout.write(`${enabled}\n`)
assert.deepEqual(decode(enabled, { cyclicDiscriminatedArrays: true }), value)

const ineligible = {
  events: [
    { type: 'login', id: 'evt_1' },
    { type: 'login', id: 'evt_2' },
    { type: 'logout', id: 'evt_3' },
  ],
}
assert.equal(encode(ineligible, { cyclicDiscriminatedArrays: true }), encode(ineligible))
```
```console
events:
  order: cycle(login,purchase,logout)*4
  discriminator: type
  rows: 12
  login[4|]{payload.id}:
    evt_1
    evt_4
    evt_7
    evt_10
  purchase[4|]{payload.id}:
    evt_2
    evt_5
    evt_8
    evt_11
  logout[4|]{payload.id}:
    evt_3
    evt_6
    evt_9
    evt_12
```

- `delimiter` selects the active delimiter for array and tabular headers: comma, pipe, or tab. Spec: [Delimiter choice](../../docs/proposals/delimiter-choice.md).

Default output, comma-delimited:

```js
import { encode } from '@reddb-io/toon'

const value = { rows: [{ id: 1, name: 'Ada' }] }
process.stdout.write(`${encode(value)}\n`)
```
```console
rows[1]{id,name}:
  1,Ada
```

The same value with `delimiter: '|'` — the header itself declares the active delimiter, so the document stays self-describing:

```js
import assert from 'node:assert/strict'
import { decode, encode } from '@reddb-io/toon'

const value = { rows: [{ id: 1, name: 'Ada' }] }
const pipe = encode(value, { delimiter: '|' })
process.stdout.write(`${pipe}\n`)
assert.deepEqual(decode(pipe), value)
```
```console
rows[1|]{id|name}:
  1|Ada
```

## TOONL Streams

TOONL is a line-oriented stream profile for flat records. A segment opens with a schema header, appends one row per line, and may close with a `[=N]` trailer. TOONL v0.2 adds resumable cursors, header-preserving trim semantics, tagged multiplexing, close-transform variants, and append-safe retry patterns. See [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md).

```js
import { closeTransform, decodeLines, encodeToonlLines } from '@reddb-io/toon'

const emitter = encodeToonlLines()
let stream = ''
stream += emitter.push({ id: 1, name: 'Ada' })
stream += emitter.push({ id: 2, name: 'Linus' })
stream += emitter.end()

for await (const record of decodeLines(stream)) {
  console.log(record.name)
}

console.log(JSON.stringify(closeTransform(stream)))
```
```console
Ada
Linus
```
```console
["[2]{id,name}:\n  1,Ada\n  2,Linus\n"]
```

- `ToonlEncoder` builds one fixed-schema segment from already encoded cells (`pushRawRow`) or flat records (`pushRow`) and closes it with `finish()`.
- `ToonlReader` is an async iterable over records from a string, `Uint8Array`, iterable, or async iterable. Its `cursor` property exposes the current resumable cursor; constructing with `{ cursor }` resumes from a prior cursor and throws `ToonlCursorInvalidationError` when the input was truncated or its anchor no longer matches.
- `ToonlDecodeStream()` is a WHATWG `TransformStream` from TOONL text or bytes to records.
- `ToonlEncodeStream(options?)` is a WHATWG `TransformStream` from records to TOONL text.
- `decodeLines(source)` is the async-generator form of the decoder. It follows schema rotation, skips blank lines, validates trailers, and supports strings plus sync or async chunk iterables.
- `encodeToonlLines(options?)` returns an incremental emitter with `push(record)`, `declareLane(tag, fields)`, `pushTagged(tag, record)`, and `end()`. Options are `delimiter`, `trailer`, `continuationEveryRows`, and `continuationEveryBytes`.
- `encodeRecords(records, options?)` buffers an iterable of records into one TOONL string, rotating segments when record shape changes.
- `parseStream(input)` returns raw segments with decoded headers and raw cells; `parseRecords(input)` returns decoded records.
- Cursors record byte offset, active header, row count since that header, and optional anchor bytes. They support append-safe resume and are invalidated by truncation or anchor mismatch.
- Trim is the TOONL v0.2 header-preserving suffix operation. The JS package exposes the stream semantics through cursor-safe reading and close transforms; the CLI command is documented in the `tq` README.
- Tagged multiplexing uses `declareLane(tag, fields)` and `pushTagged(tag, record)` to interleave multiple schemas in one append-only stream.
- `closeTransform(input)` closes TOONL into one canonical TOON document per lane segment.
- `closeTransformInterleaved(input)` closes tagged streams while preserving row-run interleaving for post-mortem rendering.
- `recordTransform(fn, options?)` maps or filters record streams and emits TOONL. Return `undefined` or `null` to drop a record.
- `JsonlToToonl(options?)` and `ToonlToJsonl()` are line-by-line WHATWG stream bridges.
- `jsonToToon(input)` and `toonToJson(input)` are whole-document JSON and canonical TOON bridges.

### TOONL Options

- `delimiter` selects comma, pipe, or tab for the stream header and rows.

Default output, comma-delimited:

```js
import { encodeRecords } from '@reddb-io/toon'

const records = [{ id: 1, name: 'Ada' }]
process.stdout.write(encodeRecords(records))
```
```console
[]{id,name}:
1,Ada
[=1]
```

The same records with `delimiter: '|'`:

```js
import assert from 'node:assert/strict'
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const records = [{ id: 1, name: 'Ada' }]
const pipe = encodeRecords(records, { delimiter: '|' })
process.stdout.write(pipe)
assert.deepEqual(parseRecords(pipe), records)
```
```console
[|]{id|name}:
1|Ada
[=1]
```

- `trailer` defaults to `true`; set it to `false` for an append-open stream without a final `[=N]` count.

Default output, closed with a trailer:

```js
import { encodeRecords } from '@reddb-io/toon'

const records = [{ id: 1 }, { id: 2 }]
process.stdout.write(encodeRecords(records))
```
```console
[]{id}:
1
2
[=2]
```

The same records with `trailer: false` — an append-open stream:

```js
import assert from 'node:assert/strict'
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const records = [{ id: 1 }, { id: 2 }]
const open = encodeRecords(records, { trailer: false })
process.stdout.write(open)
assert.deepEqual(parseRecords(open), records)
```
```console
[]{id}:
1
2
```

- `continuationEveryRows` repeats the active header after a row cadence so a reader can resume from later chunks.

```js
import assert from 'node:assert/strict'
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const records = [{ id: 1 }, { id: 2 }, { id: 3 }]
const stream = encodeRecords(records, { continuationEveryRows: 2 })
process.stdout.write(stream)
assert.deepEqual(parseRecords(stream), records)
```
```console
[]{id}:
1
2
[~]{id}:
3
[=3]
```

- `continuationEveryBytes` repeats the active header after a byte cadence; the exact boundary is chosen between rows.

```js
import assert from 'node:assert/strict'
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const records = [{ id: 1, msg: 'alpha' }, { id: 2, msg: 'beta' }]
const stream = encodeRecords(records, { continuationEveryBytes: 8 })
process.stdout.write(stream)
assert.deepEqual(parseRecords(stream), records)
```
```console
[]{id,msg}:
1,alpha
[~]{id,msg}:
2,beta
[=2]
```

```js
import { encodeToonlLines, closeTransformInterleaved } from '@reddb-io/toon'

const stream = encodeToonlLines()
let out = ''
out += stream.declareLane('api', ['id', 'path'])
out += stream.pushTagged('api', { id: 1, path: '/health' })
out += stream.declareLane('job', ['id', 'state'])
out += stream.pushTagged('job', { id: 7, state: 'queued' })
out += stream.end()

console.log(JSON.stringify(closeTransformInterleaved(out)))
```
```console
["[1]{id,path}:\n  1,/health\n","[1]{id,state}:\n  7,queued\n"]
```

Node file helpers live in the `@reddb-io/toon/node` subpath so the main entry stays universal:

```js
import { readToonlFile, writeToonlFile } from '@reddb-io/toon/node'

await writeToonlFile('users.toonl', [{ id: 1, name: 'Ada' }])

for await (const record of readToonlFile('users.toonl')) {
  console.log(record.name)
}
```

The main entry uses standard Web Streams. In Node, bridge native streams with `Readable.toWeb(nodeReadable)` and `Readable.fromWeb(webReadable)` from `node:stream`.

## Helpers And Errors

```js
import { appendSummaryField, projectFields } from '@reddb-io/toon'

const out = appendSummaryField({ service: 'checkout', rows: 3 }, { total: 3 })
const thin = projectFields([{ id: 1, state: 'ok', debug: true }], ['id', 'state'])
```

- `appendSummaryField(value, summary)` returns one conforming TOON document with a trailing `summary:` field.
- `projectFields(rows, fields)` keeps allowlisted fields in allowlist order, drops other fields, and leaves absent fields absent.
- `ToonError` is thrown by TOON decode failures and carries the 1-based source `line`.
- `ToonlError` is thrown by TOONL decode or encode failures; `line` is `0` when there is no line context.
- `ToonlCursorInvalidationError` extends `ToonlError` for failed cursor resumes and carries `condition` plus `details`.

## License

[MIT](LICENSE).
