# @reddb-io/toon

TOON v3.3 parser and serializer, plus TOONL v0.1 append-only streaming — in dependency-free ESM.

TOON ([Token-Oriented Object Notation](https://github.com/toon-format/spec)) is JSON's data model in a layout that costs far fewer tokens to hand to a model. This package decodes TOON to plain JSON values and encodes them back to canonical TOON, and it runs the **official spec corpus** — all 389 fixtures, no exceptions — as part of its test suite, alongside the Rust implementation that powers the [`tq`](https://github.com/reddb-io/toon) CLI.

Zero dependencies, no build step, hand-written types.

```bash
pnpm add @reddb-io/toon
```

## TOON

```js
import { parse, serialize } from '@reddb-io/toon'

const document = parse('users[2]{id,name}:\n  1,Ada\n  2,Linus\n')
// → { users: [{ id: 1, name: 'Ada' }, { id: 2, name: 'Linus' }] }

serialize(document)
// → 'users[2]{id,name}:\n  1,Ada\n  2,Linus\n'
```

- `parse(input, options?)` — decode to a JSON value. Options are the spec's decoder options: `indent` (default `2`), `strict` (default `true`), `expandPaths` (`'safe'` expands dotted keys into nested objects), and `maxDepth` (default `1000`; set `0` only for trusted input to disable the nesting guard).
- `parseDocument(input, options?)` — the same, but the root must be an object.
- `serialize(value)` — encode as canonical TOON: comma delimiter, two-space indent, no key folding, and the same `maxDepth` guard.
- `serialize(value, { keyedMapCollapse: true })` — opt into keyed-map collapse for uniform object maps. The encoder emits `map{fields}:` only for deterministic uniform maps: at least two entries, every value is a non-empty object, all entries share the first entry's key set, and all header leaves are primitive unless `nestedTabularHeaders` is also enabled. Non-uniform maps fall back to ordinary object form.

Failures throw a `ToonError` carrying the 1-based source `line`.

## TOONL

TOONL is the streaming profile: an append-only sequence of segments, each opened by a header, filled one row per line, and optionally closed by a `[=N]` trailer asserting the row count. No length is known up front, so a writer can keep appending forever.

```js
import { closeTransform, decodeLines, encodeLines } from '@reddb-io/toon'

const emitter = encodeLines()
let stream = ''
stream += emitter.push({ id: 1, name: 'Ada' })   // writes the header lazily
stream += emitter.push({ id: 2, name: 'Linus' })
stream += emitter.end()                          // '[=2]\n'

for await (const record of decodeLines(stream)) {
  console.log(record.name)
}

closeTransform(stream)
// → ['[2]{id,name}:\n  1,Ada\n  2,Linus\n'] — one canonical TOON document per segment
```

- `encodeLines(options?)` — incremental emitter. The header is written lazily with the first record, a schema change rotates the segment automatically, and `end()` closes the last one. Field order is canonicalized per record shape using the first order seen for that shape, so shuffled object keys do not force a rotation. `delimiter` defaults to `','`; `trailer: false` leaves the stream trailer-free.
- `decodeLines(source)` — async generator, one record per row. Takes a string or an (async) iterable of chunks, so a socket or file stream flows straight through. Follows schema rotation, skips blank lines, and checks every trailer against the rows actually seen.
- `closeTransform(input)` — closes the stream in the default per-lane form: each lane segment becomes one length-bearing canonical TOON document.
- `closeTransformInterleaved(input)` — closes a multiplexed stream while preserving row-run interleaving for post-mortem rendering.
- `ToonlDecodeStream()` / `ToonlEncodeStream(options?)` — Web Streams API transforms for `string | Uint8Array` TOONL chunks and record objects.
- `JsonlToToonl(options?)` / `ToonlToJsonl()` — line-by-line bridges between JSONL and TOONL.
- `recordTransform(fn, options?)` — maps or filters records and emits TOONL, preserving schema rotation in the output stream. Return `undefined` or `null` to drop a record.
- `jsonToToon(input)` / `toonToJson(input)` — whole-document bridges for JSON and canonical TOON.
- `encodeRecords(records, options?)`, `parseStream(input)`, `parseRecords(input)`, and the `ToonlEncoder` class cover the buffered cases.

Node file helpers live in the `@reddb-io/toon/node` subpath so the main entry stays universal:

```js
import { readToonlFile, writeToonlFile } from '@reddb-io/toon/node'

await writeToonlFile('users.toonl', [{ id: 1, name: 'Ada' }])

for await (const record of readToonlFile('users.toonl')) {
  console.log(record.name)
}
```

The main entry uses standard Web Streams. In Node, bridge native streams with `Readable.toWeb(nodeReadable)` and `Readable.fromWeb(webReadable)` from `node:stream`.

Failures throw a `ToonlError` (`line` is `0` when there is no line context).

## Helpers

Consumer-facing conveniences layered on the codec (`encode`/`decode` are exact
aliases of `serialize`/`parse` for consumers that speak that vocabulary):

```js
import { appendSummaryField, projectFields } from '@reddb-io/toon'

// One conforming document with a trailing spec-legal `summary:` field —
// parse(out) recovers the rollup with the rest of the payload.
const out = appendSummaryField({ service: 'checkout', rows: 3 }, { total: 3 })

// Rows projected onto a minimal schema: allowlist order kept, other fields
// dropped, absent fields left absent (never null-filled).
const thin = projectFields(rows, ['id', 'state'])
```

## License


[MIT](LICENSE).
