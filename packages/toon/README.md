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

- `parse(input, options?)` — decode to a JSON value. Options are the spec's decoder options: `indent` (default `2`), `strict` (default `true`), `expandPaths` (`'safe'` expands dotted keys into nested objects).
- `parseDocument(input, options?)` — the same, but the root must be an object.
- `serialize(value)` — encode as canonical TOON: comma delimiter, two-space indent, no key folding.

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

- `encodeLines(options?)` — incremental emitter. The header is written lazily with the first record, a schema change rotates the segment automatically, and `end()` closes the last one. `delimiter` defaults to `','`; `trailer: false` leaves the stream trailer-free.
- `decodeLines(source)` — async generator, one record per row. Takes a string or an (async) iterable of chunks, so a socket or file stream flows straight through. Follows schema rotation, skips blank lines, and checks every trailer against the rows actually seen.
- `closeTransform(input)` — closes the stream: each segment becomes one length-bearing canonical TOON document.
- `encodeRecords(records, options?)`, `parseStream(input)`, `parseRecords(input)`, and the `ToonlEncoder` class cover the buffered cases.

Failures throw a `ToonlError` (`line` is `0` when there is no line context).

## License

[MIT](LICENSE).
