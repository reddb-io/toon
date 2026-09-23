# Prompting LLMs with TOON

TOON is meant to sit inside prompts, so these practices apply on both sides:
giving models data to read, and asking them to write data back. For the syntax
itself see the [cheatsheet](cheatsheet.md); for measured accuracy and token
counts see [`benchmarks/`](../benchmarks/README.md).

## Show the format, don't explain it

Models read TOON without an introduction. Put the data in a fenced `toon`
block and ask the question; an explanation of the grammar costs tokens and
rarely helps. If a model has never seen TOON, one sentence is enough:

> Data is in TOON: `key: value` lines, and `name[N]{fields}:` tables with one
> row per item.

Prefer the shapes TOON compresses well. An array of uniform objects becomes
one header plus comma-separated rows, which is where most of the saving over
JSON comes from. Deeply nested, irregular data saves little; send it as JSON
if the model struggles.

## Ask for output with a header template

When you want TOON back, give the exact header and let the model fill in the
rows. The declared length is what lets you detect a truncated answer, so ask
for it explicitly:

````text
Return the matching users as TOON, in exactly this shape, with N set to the
number of rows you return and nothing outside the code block:

```toon
users[N]{id,name,role}:
  <id>,<name>,<role>
```
````

- Name every field in the header, in the order you want.
- Keep values simple: say that values containing a comma must be quoted, or
  ask for the tab delimiter (`users[N<TAB>]{id<TAB>name}:`) when free text is
  common.
- Ask for one document per answer. Several top-level documents in one reply
  are harder to validate than one object with several keys.

## Validate, then retry with the error

Decode strictly, bound the input, and treat a failure as feedback for one more
attempt:

```js
import { decode, detectTruncation } from '@reddb-io/toon'

/** Strips the ```toon fence models like to add. */
function unfence(text) {
  const match = text.match(/```(?:toon)?\n([\s\S]*?)\n?```/)
  return match ? match[1] : text.trim()
}

export function parseModelToon(reply) {
  const text = unfence(reply)
  try {
    const value = decode(text, { maxInputBytes: 1_000_000, maxArrayLength: 10_000, maxKeys: 1_000 })
    return { ok: true, value }
  } catch (error) {
    // A declared length the rows fall short of reads as a cut-off answer.
    if (detectTruncation(text).kind === 'array_length_mismatch') {
      return { ok: false, retry: 'Your answer was cut off. Send the complete document.' }
    }
    // `kind` is stable across versions; `line` points the model at the problem.
    return { ok: false, retry: `Line ${error.line}: ${error.reason} (${error.kind}). Fix it and resend.` }
  }
}
```

- `detectTruncation` reports `array_length_mismatch` when the rows fall short
  of the declared length, the usual sign of an answer cut off by a token limit,
  so the retry prompt can ask for the rest instead of a fix.
- Strict mode rejects duplicate keys and length mismatches instead of
  guessing; keep it on for model output.
- The limits keep a runaway answer from costing memory: a declared
  `items[4294967296]` fails immediately instead of being trusted.
- Errors carry a stable `kind` and the source `line`, which make a precise
  retry message.

## Choose the delimiter for the data

| Delimiter | Use when |
| --- | --- |
| Comma (default) | Values are numbers, identifiers and short text |
| Tab | Values often contain commas; a tab is usually a single token and never needs quoting |
| Pipe | Humans also read the prompt, and values rarely contain `\|` |

Encode with `encode(value, { delimiter: '\t' })` and show the model the same
delimiter you expect back.

## reddb-io extensions in prompts

The [extensions](toon-reddb-spec.md) shrink specific shapes further. They are
fine on the input side, but a model has to be shown the form before it can
produce it:

- **Primitive-array columns** (`tags[;]`) suit tables whose rows carry short
  lists. Show a sample row before expecting a model to reproduce the `;`
  separator; the local benchmarks do not yet measure generation in this form.
- **Object-array columns** (`items{sku,q}` with a count cell and indented child
  rows) suit orders with line items. For output, prefer official list form
  unless you have tested the model on child tables; the count cell is easy to
  get wrong.
- **Cyclic discriminated arrays** are an input-side compression for event
  streams. Don't ask a model to write them.

Decoders recognize array columns by their header shape. Cyclic arrays need
`decode(text, { cyclicDiscriminatedArrays: true })`.

## Tool lists

An MCP `tools/list` result is mostly JSON-Schema punctuation. When the tool list
goes into a prompt, `encodeToolManifest(tools)` renders it as TOON with one
tabular `params` row per argument (`name,type,required,description`). It is a
summary for the model to read. The host still validates calls against the full
`inputSchema`.

## Streams

For long or incremental output, ask for [TOONL](toonl-reddb-spec.md): the
header once, then one record per line, and a closing `[=N]` trailer. Each
complete line is usable as it arrives, and the trailer confirms that nothing was
lost.
