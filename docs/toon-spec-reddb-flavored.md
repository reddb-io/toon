# TOON — reddb-io Flavored Specification

## Acknowledgment

This document records the decisions and proposed evolutions that reddb-io layers
over **TOON**, the Token-Oriented Object Notation. TOON is the work of the
[toon-format](https://github.com/toon-format/spec) team and its author, Johann
Schopplich, released under the MIT License; we are grateful for a base
specification that is deterministic, minimally-quoted, and clean enough that our
additions can be strict, opt-in, and always backward-compatible. Nothing here
replaces TOON v3.3 or changes its meaning: the extensions below are *decode
always-on, encode opt-in*, they *fail closed* against a strict v3.3 decoder, and
they *round-trip losslessly*. The default output of every reddb-io implementation
is canonical TOON v3.3, byte-identical to a spec-only implementation. Our thanks
to the toon-format team for the standard this document builds on.

## Introduction

This is the normative specification of the TOON *dialect* implemented by this
repository — the reddb-io flavor. It answers, precisely: which official
specification we implement, what we add on top of it, and what compatibility is
guaranteed between the two. It absorbs and replaces the repository's former
standalone TOON-extensions document.

For an annotated, section-by-section companion to the official TOON v3.3 spec and
how our implementations conform to it, see [`toon-spec.md`](toon-spec.md). For our
streaming layer, see [`toonl.md`](toonl.md).

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY, and OPTIONAL are to be interpreted as described in RFC 2119.

## Baseline: the official TOON specification

The official TOON specification is `SPEC.md` in
[toon-format/spec](https://github.com/toon-format/spec) (Working Draft **v3.3**,
dated 2026-05-21), vendored in this repository as the `vendor/toon-spec` git
submodule. The submodule pin — commit
`f55b93ac489f297ff597d95e4c19ae84675eaeb7` — is the exact revision our
conformance suite runs against. Both implementations (the Rust crate
`reddb-io-toon` and the JS package `@reddb-io/toon`) pass **100% of the official
spec corpus** at that pin, and CI enforces it.

Unless explicitly stated in this document, **TOON v3.3 governs**. This document
changes no v3.3 semantics: every valid v3.3 document decodes here with identical
meaning, and the **default encoder output is canonical TOON v3.3, byte-identical
to a spec-only implementation** — the extensions below never appear in output
unless explicitly enabled.

## The extension model

Both wire extensions follow the same asymmetric rule. These four properties are
the contract of the reddb-io flavor:

- **Decoding is always on.** A decoder in this repository MUST accept the extended
  forms without any flag.
- **Encoding is opt-in.** An encoder MUST NOT emit an extended form unless the
  caller enabled it. With no options set, output is canonical v3.3.
- **Fail-closed on strict v3 decoders.** Each extended form is a syntax error for
  a spec-only v3.3 decoder — a document using them is rejected, never silently
  decoded into a different shape.
- **Lossless round-trip, unconditionally.** `decode(encode(x, opts)) == x` for
  every JSON value `x` and every combination of extension options. Values that do
  not fit an extension's eligibility rule fall back to standard v3.3 forms.

The asymmetry is deliberate: turning *decoding* on always costs nothing to a
producer that never emits the forms, while keeping *encoding* opt-in guarantees
that a naïve pipeline can never accidentally emit a document a strict v3.3 reader
would reject. Fail-closed rather than fail-open is the safety property that makes
"decode always-on" tolerable: a strict v3.3 decoder confronted with an extended
form errors loudly instead of quietly reading a different shape.

### Enabling emission, per surface

| Surface | Active delimiter | Nested tabular headers | Keyed-map collapse | Primitive-array columns |
| --- | --- | --- | --- | --- |
| JS — `serialize(value, opts)` | `{ delimiter: ',' \| '\t' \| '\|' }` | `{ nestedTabularHeaders: true }` | `{ keyedMapCollapse: true }` | `{ primitiveArrayColumns: true }` |
| Rust — `to_toon_with_options(EncodeOptions)` | `delimiter: ',' \| '\t' \| '\|'` | `nested_tabular_headers: true` | `keyed_map_collapse: true` | `primitive_array_columns: true` |
| `tq` (TOON output) | `--delimiter comma\|tab\|pipe` | `--nested-tabular-headers` | `--keyed-map-collapse` | `--primitive-array-columns` |

Delimiter choice is pure TOON v3.3 for arrays and tabular rows: encoders emit the active-delimiter declaration in the header (`[N|]`, `[N\t]`, and matching field lists) and quote cells that contain the active delimiter. The keyed-map collapse extension mirrors that declaration at the start of its field list, for example `map{|id|name}:`, so extension rows remain self-describing.

## Extension 1 — Nested tabular headers

*Origin: upstream RFC [toon-format/spec#46](https://github.com/toon-format/spec/issues/46).*

v3.3's tabular form (`key[N]{fields}:`) requires every column to be a primitive.
This extension lets a column itself be a uniform nested object, declared
recursively in the header as `field{sub1,sub2}`. Rows stay flat
delimiter-separated lines; the header alone encodes the nested shape.

```toon
orders[2]{id,customer{name,country},total}:
  1,Ada,UK,10.5
  2,Bob,US,20
```

decodes exactly as the v3.3 expanded form of:

```json
{"orders": [
  {"id": 1, "customer": {"name": "Ada", "country": "UK"}, "total": 10.5},
  {"id": 2, "customer": {"name": "Bob", "country": "US"}, "total": 20}
]}
```

Rules:

- The field-list grammar becomes recursive: a field is either a key, or a key
  followed by a braced field list (`customer{name,country}`), to any depth.
- Row arity counts **leaf** columns. A nested group consumes exactly its leaf
  count of cells per row, in header order.
- Malformed nested headers (unbalanced braces, empty groups, duplicate leaf
  paths) MUST be reported as parse errors with the header's line number.
- An encoder with the option enabled emits this form only when every record in the
  array has the same shape recursively (same key sets at every level, all leaves
  primitive). Any mismatch falls back to the standard expanded list form — never a
  hard error.

## Extension 2 — Keyed-map collapse

*Origin: upstream RFC [toon-format/spec#57](https://github.com/toon-format/spec/issues/57).*

Arrays of uniform objects get table-collapse in v3.3; keyed object *maps* with
uniform values do not, so every field name repeats once per entry. This extension
gives uniform maps the same treatment, reusing the recursive-brace header grammar
— no new sigil family:

```toon
people{first,last}:
  joe: Joe,Schmoe
  mary: Mary,Jane
```

decodes to an object map (not an array):

```json
{"people": {
  "joe":  {"first": "Joe",  "last": "Schmoe"},
  "mary": {"first": "Mary", "last": "Jane"}
}}
```

Rules:

- The header is `key{fields}:` — object-typed because there is **no `[N]`
  segment**. A strict v3.3 decoder rejects it (fail-closed) instead of reading a
  different shape.
- Each row is `mapKey: cells`, one line per entry, indented one level. Map keys in
  row position follow the standard v3.3 key-quoting rules.
- Non-uniform maps stay in the ordinary v3.3 object form. Round-trip is lossless
  in every case.
- Nested (recursive) leaves are eligible only when [nested tabular
  headers](#extension-1--nested-tabular-headers) is **also** enabled.

### The entry-count guardrail and its trade-off

Encoder eligibility is deterministic. An encoder with the option enabled emits the
keyed-map collapse form only when **all** of the following hold:

1. the object has **at least two entries**;
2. every entry value is a non-empty object;
3. every entry has the same key set as the first entry; and
4. each header leaf is primitive (or eligible per the nested-headers rule above).

Rule 1 — the **entry-count guardrail** — is the notable trade-off. A single-entry
uniform map is *representable* in the collapsed form, but it is deliberately **not
collapsed**. The reasoning is a token/clarity balance: for one entry the collapsed
header `people{first,last}:` plus one `joe: Joe,Schmoe` row does not beat the
ordinary object form on tokens, and it costs the reader a header they must parse
to understand a single record. Below two entries the collapse is not worth the
indirection, so the guardrail keeps output in the plain, self-evident object form
until there is real repetition to amortize. The trade-off is that a producer
emitting maps of size one never sees the collapsed form even with the option on;
this is intentional and keeps the encoder's output stable and predictable rather
than flipping shape at a size-one boundary. Round-trip is lossless either way,
because a non-collapsed map is just standard v3.3.

## Extension 3 — Primitive-array columns

Object-array tables often include fields such as tags, flags, or aliases whose
value is itself a short array of primitive values. Standard v3.3 cannot keep the
outer table in tabular form when a column is an array, so the whole array falls
back to expanded list items. This extension lets one table cell carry a
primitive-list payload:

```toon
items[3|]{id|tags[;]|quantity}:
  item_001|hazmat;oversize|60
  item_002||0
  item_003|"frag;ile";null;true|7
```

decodes to:

```json
{"items": [
  {"id": "item_001", "tags": ["hazmat", "oversize"], "quantity": 60},
  {"id": "item_002", "tags": [], "quantity": 0},
  {"id": "item_003", "tags": ["frag;ile", null, true], "quantity": 7}
]}
```

Grammar:

- A tabular header field may be `key[subDelimiter]`, where `subDelimiter` is a
  single character used only inside that field's cells.
- The sub-delimiter MUST be non-empty, MUST differ from the active row
  delimiter, and MUST NOT be a space, tab, newline, carriage return, quote,
  bracket, brace, or colon.
- An empty cell in a primitive-array column decodes as an empty array.
- Non-empty cells split on unquoted sub-delimiters, then each subcell decodes as
  an ordinary scalar. Strings are quoted when they contain either the active row
  delimiter or the sub-delimiter.

Encoder eligibility is deterministic. With the option enabled, an encoder emits
this form only when the containing value is otherwise eligible for tabular object
array encoding and every value in the selected column is an array whose items are
strings, numbers, booleans, or null. Empty arrays are eligible. Mixed scalar/list
columns, nested arrays, object items, non-uniform rows, and list-item position all
fall back to ordinary v3.3 output losslessly.

## Delimiter choice

TOON v3.3 supports three delimiters — comma (default), tab (HTAB), and pipe
(`|`) — selected by the encoder as the *document delimiter* and declared per
array header as the *active delimiter*. The reddb-io flavor makes no change to
this mechanism and adds no fourth delimiter; our decisions are about *defaults*
and *when to reach for a non-default*:

- **Comma is the default**, matching the official spec, because it is the most
  familiar and the most token-efficient for the common case where cell values do
  not themselves contain commas.
- **Tab** is preferred when cells routinely contain commas (free-text fields,
  locale-formatted numbers), because it avoids per-cell quoting: a value with a
  comma needs no quotes under a tab-delimited header, which usually nets fewer
  tokens than comma-plus-quotes.
- **Pipe** is offered for human-facing tables and for payloads whose cells contain
  neither pipes nor commas uniformly.

The flavor keeps the spec's rule that **absence of a delimiter symbol always means
comma**, with no inheritance from a parent header, so a nested header's delimiter
is always locally legible. Delimiter selection never changes the decoded value;
it is purely a wire-efficiency and readability lever, and the round-trip is
lossless for every choice.

## Depth guard

Neither the official spec's data model nor its strict-mode checklist bounds
nesting depth; a maliciously or accidentally deep document can drive a naïve
recursive decoder into stack exhaustion. The reddb-io flavor adds a **depth
guard** as a robustness measure that does not change any decoded value.

- Decoding is bounded by `ParseOptions::max_depth` (Rust) and the equivalent JS
  parse option; checked encoding is bounded by `EncodeOptions::max_depth`.
- **Both default to `1000`.** A document nested deeper than the guard is rejected
  with a structured error rather than crashing the process.
- Setting `max_depth` to `0` disables the guard and MUST be done **only for
  trusted input**.
- On the encode side, prefer the checked entry points
  (`try_to_canonical_toon()` / `try_to_toon_with_options(...)` in Rust) when
  encoding untrusted or user-supplied values, so a depth failure returns an
  `EncodeError` instead of overflowing.

The guard is a defense-in-depth default, not a format change: a document within
the limit decodes identically whether or not the guard is present, and the limit
is configurable for callers whose inputs are known-shallow or known-trusted.

## detectTruncation — structured completeness reports

TOON is *self-checking* in a way JSON is not: `[N]` declares a row count and
`{f1,f2}` declares a field set, so a truncated or hallucinated table is a
structural mismatch rather than silently short data. The reddb-io flavor turns
that property into a **first-class diagnostic API** that reports *why* a document
is incomplete instead of only throwing.

The same structured report is exposed identically across all three surfaces:

- **`tq check [-p toon|toonl] [FILE]`** — prints the report and exits non-zero
  when TOON guardrails prove the input is truncated.
- **Rust** — `detect_truncation_with_options(input, options)` for TOON and
  `detect_toonl_truncation(input)` for TOONL.
- **JS** — `detectTruncation(input, { format: 'toon' | 'toonl' })`.

The report fields are stable across the CLI, the crate, and the package:
`complete`, `kind`, `line`, `declared`, `actual`, and `message`. For example, a
tabular array that declares two rows but carries one:

```json
{
  "complete": false,
  "kind": "array_length_mismatch",
  "line": 2,
  "declared": 2,
  "actual": 1,
  "message": "declared 2 rows but received 1"
}
```

This is a diagnosis, not a decode: callers that need to know whether an
LLM-produced document was cut off — before deciding to retry, extend, or reject —
get a machine-readable answer with a line number and the declared-vs-actual
counts, without catching a decode exception and re-deriving the cause.

## The wire-efficiency program

The reddb-io flavor treats token efficiency as a measured program, not a slogan.
The corpora under `tests/wire-efficiency/corpora.json` and the reproducible
benchmark harness (`scripts/research_token_benchmark.py`) keep the numbers honest
and let anyone reproduce them. All figures below are tokenized with `o200k_base`
(the GPT-4o/GPT-5 encoding, via `tiktoken`).

### TOON vs JSON — a uniform table

On a representative payload — a small object with a four-row uniform array of
five-field deploy records:

| Encoding | Tokens | Bytes |
| --- | ---: | ---: |
| JSON, pretty-printed | 200 | 569 |
| JSON, minified | 114 | 353 |
| **TOON** | **91** | **189** |

That is **54% fewer tokens than pretty-printed JSON** and **20% fewer than
minified JSON** on this payload with this tokenizer. The saving is not a universal
constant: it **grows with the number of rows** in a uniform array (the header is
amortized) and **shrinks toward zero** for deeply nested, non-uniform data where
TOON has nothing to collapse. Measure your own payload before quoting a number.

### TOONL vs JSONL — at stream scale

At **10,000 rows**, the append-only streaming layer
([`toonl.md`](toonl.md)) measures, from the shared corpora:

| Payload class | JSONL tokens | TOONL tokens | Saving |
| --- | ---: | ---: | ---: |
| Analytics export | 535,576 | 305,604 | **−42.9%** |
| Flat log | 552,500 | 360,024 | **−34.8%** |
| Envelope (opaque payload cell) | 990,000 | 906,686 | **−8.4%** |

The saving grows with stream length (the header amortizes), and open TOONL even
beats closed TOON by a point or two on the same rows — there is no `[N]` to pay
for while the stream is open. The envelope case, where each record carries one
opaque JSON payload cell that TOONL cannot collapse, marks the low end of the
range: the format saves on structure, so a payload with almost no repeated
structure saves the least.

## Relationship to the streaming layer

TOONL ([`toonl.md`](toonl.md)) is an independent line-oriented streaming
extension with its own versioning; it is unaffected by this document. The
TOONL close-transform continues to target canonical TOON v3.3 documents and does
**not** emit the nested-tabular-header, keyed-map-collapse, or
primitive-array-column forms defined here.
The two concerns compose cleanly but are specified separately.

## Conformance

The shared corpora under `tests/` pin both implementations to identical behavior:

- `tests/toon/fixtures/` (live from the `vendor/toon-spec` submodule) — the v3.3
  baseline, run by both the Rust crate and the JS package.
- The extension corpora — encode bytes and decode values for nested tabular
  headers, keyed-map collapse, and primitive-array columns, including the
  eligibility and fail-closed cases.
- `tests/json-limits/corpus.json` — the shared JSON edge corpus (numbers at the
  boundaries of the safe-integer range, precision, and other parser limits) run
  identically by the JS package and the Rust crate.
- The `tq` golden tests cover the `--nested-tabular-headers`,
  `--keyed-map-collapse`, and `--primitive-array-columns` flags end-to-end.

CI enforces the whole set on every change, so the two implementations cannot
disagree about the flavor.
