# @reddb-io/toon

> **Attribution:** This is RedDB's TypeScript implementation of TOON - not the original project. The TOON format was created by Johann Schopplich; see the [official repo](https://github.com/toon-format/toon), [toon-format/spec](https://github.com/toon-format/spec), and [toonformat.dev](https://toonformat.dev) for the format spec and original project.

TOON v3.3 parser and serializer, plus TOONL v0.2 append-only streaming, in dependency-free ESM.

TOON ([Token-Oriented Object Notation](https://github.com/toon-format/spec)) is JSON's data model in a compact model-facing layout. This package decodes TOON to plain JSON values and encodes them back to canonical TOON. It also implements the reddb-io opt-in extensions specified in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md) and the TOONL streaming layer specified in [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md).

Zero dependencies, no build step, hand-written types. Performance notes and token-efficiency measurements live in [`benchmarks/`](../../benchmarks/README.md), not in this package README.

```bash
pnpm add @reddb-io/toon
```

## TOON

```js
import { parse, serialize } from '@reddb-io/toon'

const document = parse('users[2]{id,name}:\n  1,Ada\n  2,Linus\n')
console.log(JSON.stringify(document))

process.stdout.write(serialize(document))
console.log('round-trip', JSON.stringify(parse(serialize(document))) === JSON.stringify(document))
```
```console
{"users":[{"id":1,"name":"Ada"},{"id":2,"name":"Linus"}]}
users[2]{id,name}:
  1,Ada
  2,Linus
round-trip true
```

- `parse(input, options?)` decodes a TOON document to a JSON value. Options are `indent` (default `2`), `strict` (default `true`), `expandPaths` (`'safe'` expands dotted keys into nested objects), and `maxDepth` (default `1000`; set `0` only for trusted input).
- `parseDocument(input, options?)` is the object-root variant and throws when the root is not an object.
- `serialize(value, options?)` encodes canonical TOON by default: comma delimiter, two-space indent, no key folding, and the same depth guard.
- `encode` and `decode` are exact aliases of `serialize` and `parse`.
- `detectTruncation(input, { format?: 'toon' | 'toonl', ...parseOptions })` returns a structured completeness report instead of throwing. Complete input reports `complete: true`; truncated TOON arrays, cut nested bodies, TOONL trailer mismatches, and missing TOONL trailers report `kind`, `line`, `declared`, `actual`, and `message`.

Strict mode is on by default. It enforces the official TOON error checklist; pass `{ strict: false }` only when you intentionally want legacy recovery behavior.

### Base Options

- `indent` changes how the parser interprets leading spaces. Serialization stays canonical TOON v3.3 with two-space indentation by default.

```js
import { parse, serialize } from '@reddb-io/toon'

const input = 'person:\n    city: London\n'

try {
  parse(input)
} catch (error) {
  console.log('default indent', error.message)
}

const document = parse(input, { indent: 4 })
console.log('indent 4', JSON.stringify(document))
process.stdout.write(serialize(document))
console.log('round-trip', JSON.stringify(parse(serialize(document))) === JSON.stringify(document))
```
```console
default indent line 2: invalid indentation
indent 4 {"person":{"city":"London"}}
person:
  city: London
round-trip true
```

- `strict` is on by default. Turning it off keeps legacy last-write-wins recovery for duplicate keys.

```js
import { parse } from '@reddb-io/toon'

const input = 'a: 1\na: 2\n'

try {
  parse(input)
} catch (error) {
  console.log('strict default', error.message)
}

console.log('strict false', JSON.stringify(parse(input, { strict: false })))
```
```console
strict default line 2: duplicate key
strict false {"a":2}
```

- `maxDepth` guards both parse and serialize recursion. Set `0` only when the input is trusted and you intentionally want to disable the depth guard.

```js
import { parse, serialize } from '@reddb-io/toon'

const input = 'a:\n  b:\n    c: 1\n'

try {
  parse(input, { maxDepth: 1 })
} catch (error) {
  console.log('maxDepth 1', error.message)
}

const document = parse(input, { maxDepth: 0 })
console.log('maxDepth 0', JSON.stringify(document))
process.stdout.write(serialize(document, { maxDepth: 0 }))
console.log('round-trip', JSON.stringify(parse(serialize(document))) === JSON.stringify(document))
```
```console
maxDepth 1 line 3: maximum nesting depth exceeded (maxDepth 1)
maxDepth 0 {"a":{"b":{"c":1}}}
a:
  b:
    c: 1
round-trip true
```

### Encode Extensions

All reddb-io extensions decode always-on and encode opt-in. With no options, output remains canonical TOON v3.3. The extension model is specified in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md).

- `nestedTabularHeaders` emits recursive table headers for uniform nested object columns. Spec: [Nested tabular headers](../../docs/proposals/nested-tabular-headers.md).

```js
import { parse, serialize } from '@reddb-io/toon'

const value = { orders: [{ id: 1, customer: { name: 'Ada', country: 'UK' }, total: 10.5 }] }
const enabled = serialize(value, { nestedTabularHeaders: true })

console.log('default canonical v3.3')
process.stdout.write(serialize(value))
console.log('nestedTabularHeaders on')
process.stdout.write(enabled)
console.log('round-trip', JSON.stringify(parse(enabled)) === JSON.stringify(value))
```
```console
default canonical v3.3
orders[1]:
  - id: 1
    customer:
      name: Ada
      country: UK
    total: 10.5
nestedTabularHeaders on
orders[1]{id,customer{name,country},total}:
  1,Ada,UK,10.5
round-trip true
```

- `keyedMapCollapse` emits compact rows for object maps whose values are uniform objects. Spec: [Keyed-map collapse](../../docs/proposals/keyed-map-collapse.md).

```js
import { parse, serialize } from '@reddb-io/toon'

const value = {
  people: {
    joe: { first: 'Joe', last: 'Schmoe' },
    mary: { first: 'Mary', last: 'Jane' },
  },
}
const enabled = serialize(value, { keyedMapCollapse: true })

console.log('default canonical v3.3')
process.stdout.write(serialize(value))
console.log('keyedMapCollapse on')
process.stdout.write(enabled)
console.log('round-trip', JSON.stringify(parse(enabled)) === JSON.stringify(value))
```
```console
default canonical v3.3
people:
  joe:
    first: Joe
    last: Schmoe
  mary:
    first: Mary
    last: Jane
keyedMapCollapse on
people{first,last}:
  joe: Joe,Schmoe
  mary: Mary,Jane
round-trip true
```

- `primitiveArrayColumns` emits primitive list columns such as `tags[;]` inside otherwise tabular object arrays. Spec: [Primitive-array columns](../../docs/proposals/primitive-array-columns.md).
  By default, or when a row is not eligible, output falls back losslessly to canonical TOON v3.3.

```js
import { parse, serialize } from '@reddb-io/toon'

const value = { users: [{ id: 1, tags: ['red', 'blue'] }] }
const ineligible = { users: [{ id: 1, tags: null }, { id: 2, tags: ['ok'] }] }
const enabled = serialize(value, { primitiveArrayColumns: true })
const fallback = serialize(ineligible, { primitiveArrayColumns: true })

console.log('default canonical v3.3')
process.stdout.write(serialize(value))
console.log('primitiveArrayColumns on')
process.stdout.write(enabled)
console.log('ineligible fallback')
process.stdout.write(fallback)
console.log('fallback is canonical', fallback === serialize(ineligible))
console.log('round-trip', JSON.stringify(parse(enabled)) === JSON.stringify(value))
```
```console
default canonical v3.3
users[1]:
  - id: 1
    tags[2]: red,blue
primitiveArrayColumns on
users[1]{id,tags[;]}:
  1,red;blue
ineligible fallback
users[2]:
  - id: 1
    tags: null
  - id: 2
    tags[1]: ok
fallback is canonical true
round-trip true
```

- `objectArrayColumns` emits child tables for array-valued object columns. Spec: [Child tables and matrix](../../docs/proposals/child-tables-and-matrix.md).
  By default, or when a child array is not eligible, output falls back losslessly to canonical TOON v3.3.

```js
import { parse, serialize } from '@reddb-io/toon'

const value = { orders: [{ id: 1, items: [{ sku: 'A', qty: 2 }, { sku: 'B', qty: 1 }] }] }
const ineligible = { orders: [{ id: 1, items: [{ sku: 'A' }] }, { id: 2, items: [1] }] }
const enabled = serialize(value, { objectArrayColumns: true })
const fallback = serialize(ineligible, { objectArrayColumns: true })

console.log('default canonical v3.3')
process.stdout.write(serialize(value))
console.log('objectArrayColumns on')
process.stdout.write(enabled)
console.log('ineligible fallback')
process.stdout.write(fallback)
console.log('fallback is canonical', fallback === serialize(ineligible))
console.log('round-trip', JSON.stringify(parse(enabled)) === JSON.stringify(value))
```
```console
default canonical v3.3
orders[1]:
  - id: 1
    items[2]{sku,qty}:
      A,2
      B,1
objectArrayColumns on
orders[1]{id,items{sku,qty}}:
  1,2
    A,2
    B,1
ineligible fallback
orders[2]:
  - id: 1
    items[1]{sku}:
      A
  - id: 2
    items[1]: 1
fallback is canonical true
round-trip true
```

- `cyclicDiscriminatedArrays` emits the specialized wire for eligible top-level event arrays whose discriminator values repeat in a stable cycle. Spec: [Cyclic discriminated arrays](../../docs/proposals/cyclic-discriminated-arrays.md).
  By default, or when the discriminator order is not eligible, output falls back losslessly to canonical TOON v3.3.

```js
import { parse, serialize } from '@reddb-io/toon'

const value = { events: [] }
for (let index = 1; index <= 12; index += 1) {
  const type = ['login', 'purchase', 'logout'][(index - 1) % 3]
  value.events.push({ type, payload: { id: `evt_${index}` } })
}

const ineligible = {
  events: [
    { type: 'login', id: 'evt_1' },
    { type: 'login', id: 'evt_2' },
    { type: 'logout', id: 'evt_3' },
  ],
}
const enabled = serialize(value, { cyclicDiscriminatedArrays: true })
const fallback = serialize(ineligible, { cyclicDiscriminatedArrays: true })

console.log('default canonical v3.3')
process.stdout.write(serialize(value))
console.log('cyclicDiscriminatedArrays on')
process.stdout.write(enabled)
console.log('ineligible fallback')
process.stdout.write(fallback)
console.log('fallback is canonical', fallback === serialize(ineligible))
console.log('round-trip', JSON.stringify(parse(enabled)) === JSON.stringify(value))
```
```console
default canonical v3.3
events[12]:
  - type: login
    payload:
      id: evt_1
  - type: purchase
    payload:
      id: evt_2
  - type: logout
    payload:
      id: evt_3
  - type: login
    payload:
      id: evt_4
  - type: purchase
    payload:
      id: evt_5
  - type: logout
    payload:
      id: evt_6
  - type: login
    payload:
      id: evt_7
  - type: purchase
    payload:
      id: evt_8
  - type: logout
    payload:
      id: evt_9
  - type: login
    payload:
      id: evt_10
  - type: purchase
    payload:
      id: evt_11
  - type: logout
    payload:
      id: evt_12
cyclicDiscriminatedArrays on
@toon-cyclic-discriminated-array/1
@root {"events":"$C0"}
@array $C0 discr=type n=12 common= order=cycle(login,purchase,logout)*4
@group login n=4
{"payload":{"id":"evt_1"}}
{"payload":{"id":"evt_4"}}
{"payload":{"id":"evt_7"}}
{"payload":{"id":"evt_10"}}
@group purchase n=4
{"payload":{"id":"evt_2"}}
{"payload":{"id":"evt_5"}}
{"payload":{"id":"evt_8"}}
{"payload":{"id":"evt_11"}}
@group logout n=4
{"payload":{"id":"evt_3"}}
{"payload":{"id":"evt_6"}}
{"payload":{"id":"evt_9"}}
{"payload":{"id":"evt_12"}}
@end
ineligible fallback
events[3]{type,id}:
  login,evt_1
  login,evt_2
  logout,evt_3
fallback is canonical true
round-trip true
```

- `delimiter` selects the active delimiter for array and tabular headers: comma, pipe, or tab. Spec: [Delimiter choice](../../docs/proposals/delimiter-choice.md).

```js
import { parse, serialize } from '@reddb-io/toon'

const value = { rows: [{ id: 1, name: 'Ada' }] }
const pipe = serialize(value, { delimiter: '|' })

console.log('default comma delimiter')
process.stdout.write(serialize(value))
console.log('pipe delimiter')
process.stdout.write(pipe)
console.log('round-trip', JSON.stringify(parse(pipe)) === JSON.stringify(value))
```
```console
default comma delimiter
rows[1]{id,name}:
  1,Ada
pipe delimiter
rows[1|]{id|name}:
  1|Ada
round-trip true
```

## TOONL Streams

TOONL is a line-oriented stream profile for flat records. A segment opens with a schema header, appends one row per line, and may close with a `[=N]` trailer. TOONL v0.2 adds resumable cursors, header-preserving trim semantics, tagged multiplexing, close-transform variants, and append-safe retry patterns. See [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md).

```js
import { closeTransform, decodeLines, encodeLines } from '@reddb-io/toon'

const emitter = encodeLines()
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
["[2]{id,name}:\n  1,Ada\n  2,Linus\n"]
```

- `ToonlEncoder` builds one fixed-schema segment from already encoded cells (`pushRawRow`) or flat records (`pushRow`) and closes it with `finish()`.
- `ToonlReader` is an async iterable over records from a string, `Uint8Array`, iterable, or async iterable. Its `cursor` property exposes the current resumable cursor; constructing with `{ cursor }` resumes from a prior cursor and throws `ToonlCursorInvalidationError` when the input was truncated or its anchor no longer matches.
- `ToonlDecodeStream()` is a WHATWG `TransformStream` from TOONL text or bytes to records.
- `ToonlEncodeStream(options?)` is a WHATWG `TransformStream` from records to TOONL text.
- `decodeLines(source)` is the async-generator form of the decoder. It follows schema rotation, skips blank lines, validates trailers, and supports strings plus sync or async chunk iterables.
- `encodeLines(options?)` returns an incremental emitter with `push(record)`, `declareLane(tag, fields)`, `pushTagged(tag, record)`, and `end()`. Options are `delimiter`, `trailer`, `continuationEveryRows`, and `continuationEveryBytes`.
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

```js
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const records = [{ id: 1, name: 'Ada' }]
const pipe = encodeRecords(records, { delimiter: '|' })

console.log('default delimiter')
process.stdout.write(encodeRecords(records))
console.log('pipe delimiter')
process.stdout.write(pipe)
console.log('decodes back', JSON.stringify(parseRecords(pipe)) === JSON.stringify(records))
```
```console
default delimiter
[]{id,name}:
1,Ada
[=1]
pipe delimiter
[|]{id|name}:
1|Ada
[=1]
decodes back true
```

- `trailer` defaults to `true`; set it to `false` for an append-open stream without a final `[=N]` count.

```js
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const records = [{ id: 1 }, { id: 2 }]
const open = encodeRecords(records, { trailer: false })

console.log('with trailer by default')
process.stdout.write(encodeRecords(records))
console.log('without trailer option')
process.stdout.write(open)
console.log('decodes back', JSON.stringify(parseRecords(open)) === JSON.stringify(records))
```
```console
with trailer by default
[]{id}:
1
2
[=2]
without trailer option
[]{id}:
1
2
decodes back true
```

- `continuationEveryRows` repeats the active header after a row cadence so a reader can resume from later chunks.

```js
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const records = [{ id: 1 }, { id: 2 }, { id: 3 }]
const stream = encodeRecords(records, { continuationEveryRows: 2 })

process.stdout.write(stream)
console.log('decodes back', JSON.stringify(parseRecords(stream)) === JSON.stringify(records))
```
```console
[]{id}:
1
2
[~]{id}:
3
[=3]
decodes back true
```

- `continuationEveryBytes` repeats the active header after a byte cadence; the exact boundary is chosen between rows.

```js
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const records = [{ id: 1, msg: 'alpha' }, { id: 2, msg: 'beta' }]
const stream = encodeRecords(records, { continuationEveryBytes: 8 })

process.stdout.write(stream)
console.log('decodes back', JSON.stringify(parseRecords(stream)) === JSON.stringify(records))
```
```console
[]{id,msg}:
1,alpha
[~]{id,msg}:
2,beta
[=2]
decodes back true
```

```js
import { encodeLines, closeTransformInterleaved } from '@reddb-io/toon'

const stream = encodeLines()
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
- `ToonError` is thrown by TOON parse failures and carries the 1-based source `line`.
- `ToonlError` is thrown by TOONL decode or encode failures; `line` is `0` when there is no line context.
- `ToonlCursorInvalidationError` extends `ToonlError` for failed cursor resumes and carries `condition` plus `details`.

## License

[MIT](LICENSE).
