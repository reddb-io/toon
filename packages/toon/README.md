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
// { users: [{ id: 1, name: 'Ada' }, { id: 2, name: 'Linus' }] }

serialize(document)
// 'users[2]{id,name}:\n  1,Ada\n  2,Linus\n'
```

- `parse(input, options?)` decodes a TOON document to a JSON value. Options are `indent` (default `2`), `strict` (default `true`), `expandPaths` (`'safe'` expands dotted keys into nested objects), and `maxDepth` (default `1000`; set `0` only for trusted input).
- `parseDocument(input, options?)` is the object-root variant and throws when the root is not an object.
- `serialize(value, options?)` encodes canonical TOON by default: comma delimiter, two-space indent, no key folding, and the same depth guard.
- `encode` and `decode` are exact aliases of `serialize` and `parse`.
- `detectTruncation(input, { format?: 'toon' | 'toonl', ...parseOptions })` returns a structured completeness report instead of throwing. Complete input reports `complete: true`; truncated TOON arrays, cut nested bodies, TOONL trailer mismatches, and missing TOONL trailers report `kind`, `line`, `declared`, `actual`, and `message`.

Strict mode is on by default. It enforces the official TOON error checklist; pass `{ strict: false }` only when you intentionally want legacy recovery behavior.

### Encode Extensions

All reddb-io extensions decode always-on and encode opt-in. With no options, output remains canonical TOON v3.3. The extension model is specified in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md).

- `nestedTabularHeaders` emits recursive table headers for uniform nested object columns. Spec: [Nested tabular headers](../../docs/proposals/nested-tabular-headers.md).

  ```js
  import { serialize } from '@reddb-io/toon'

  serialize(
    { orders: [{ id: 1, customer: { name: 'Ada', country: 'UK' }, total: 10.5 }] },
    { nestedTabularHeaders: true },
  )
  // 'orders[1]{id,customer{name,country},total}:\n  1,Ada,UK,10.5\n'
  ```

- `keyedMapCollapse` emits compact rows for object maps whose values are uniform objects. Spec: [Keyed-map collapse](../../docs/proposals/keyed-map-collapse.md).

  ```js
  import { serialize } from '@reddb-io/toon'

  serialize(
    { people: { joe: { first: 'Joe', last: 'Schmoe' }, mary: { first: 'Mary', last: 'Jane' } } },
    { keyedMapCollapse: true },
  )
  // 'people{first,last}:\n  joe: Joe,Schmoe\n  mary: Mary,Jane\n'
  ```

- `primitiveArrayColumns` emits primitive list columns such as `tags[;]` inside otherwise tabular object arrays. Spec: [Primitive-array columns](../../docs/proposals/primitive-array-columns.md).

  ```js
  import { serialize } from '@reddb-io/toon'

  serialize({ users: [{ id: 1, tags: ['red', 'blue'] }] }, { primitiveArrayColumns: true })
  // 'users[1]{id,tags[;]}:\n  1,red;blue\n'
  ```

- `objectArrayColumns` emits child tables for array-valued object columns. Spec: [Child tables and matrix](../../docs/proposals/child-tables-and-matrix.md).

  ```js
  import { serialize } from '@reddb-io/toon'

  serialize(
    { orders: [{ id: 1, items: [{ sku: 'A', qty: 2 }, { sku: 'B', qty: 1 }] }] },
    { objectArrayColumns: true },
  )
  // 'orders[1]{id,items{sku,qty}}:\n  1,2\n    A,2\n    B,1\n'
  ```

- `cyclicDiscriminatedArrays` emits the specialized wire for eligible top-level event arrays whose discriminator values repeat in a stable cycle. Spec: [Cyclic discriminated arrays](../../docs/proposals/cyclic-discriminated-arrays.md).

  ```js
  import { serialize } from '@reddb-io/toon'

  serialize(
    [
      { type: 'request', id: 1 },
      { type: 'response', id: 1 },
      { type: 'request', id: 2 },
      { type: 'response', id: 2 },
    ],
    { cyclicDiscriminatedArrays: true },
  )
  ```

- `delimiter` selects the active delimiter for array and tabular headers: comma, pipe, or tab. Spec: [Delimiter choice](../../docs/proposals/delimiter-choice.md).

  ```js
  import { serialize } from '@reddb-io/toon'

  serialize({ rows: [{ id: 1, name: 'Ada' }] }, { delimiter: '|' })
  // 'rows[1|]{id|name}:\n  1|Ada\n'
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

closeTransform(stream)
// ['[2]{id,name}:\n  1,Ada\n  2,Linus\n']
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

```js
import { encodeLines, closeTransformInterleaved } from '@reddb-io/toon'

const stream = encodeLines()
let out = ''
out += stream.declareLane('api', ['id', 'path'])
out += stream.pushTagged('api', { id: 1, path: '/health' })
out += stream.declareLane('job', ['id', 'state'])
out += stream.pushTagged('job', { id: 7, state: 'queued' })
out += stream.end()

closeTransformInterleaved(out)
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
