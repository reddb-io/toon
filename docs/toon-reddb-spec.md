# TOON v4.1 — reddb-io opt-in extensions

**tl;dr.** This document specifies the userland extensions that reddb-io layers over the official **TOON v4.1** baseline. Primitive-array and object-array wires decode by default and encode only when requested. Cyclic discriminated arrays require an explicit decode or encode option because their wire is also a valid literal v4.1 object. Delimiter choice is official v4.1 configuration, while the depth guard and `detectTruncation` are local APIs rather than wire grammar. Nested field groups and keyed tabular form were absorbed by v4.1; their sections below are design history and upstream now governs them. See the [v4.1 migration notes](migration-v4.md) for the retired pre-v4 behavior. We thank the [toon-format](https://github.com/toon-format/spec) team and author Johann Schopplich for a standard clean enough to extend safely.

## Acknowledgment

This document records the decisions and proposed evolutions that reddb-io layers
over **TOON**, the Token-Oriented Object Notation. TOON is the work of the
[toon-format](https://github.com/toon-format/spec) team and its author, Johann
Schopplich, released under the MIT License; we are grateful for a base
specification that is deterministic, minimally-quoted, and clean enough that our
additions can be explicit and lossless. Nothing here replaces the official
**TOON v4.1** specification or changes its meaning. Primitive-array and
object-array wires fail closed against a strict spec-only decoder; cyclic wire
instead has a safe literal v4.1 reading and is reconstructed only on opt-in.
The default encoder output is canonical TOON v4.1. Our thanks to the toon-format
team for the standard this document builds on.

## Introduction

This is the normative specification of the opt-in *extensions* this repository
layers on the official TOON v4.1 baseline — the reddb-io flavor. It is not a
competing dialect: the baseline is plain TOON v4.1, and these extensions are
strictly additive and opt-in on encode. It answers, precisely: which official
specification we implement, what we add on top of it, and what compatibility is
guaranteed between the two. It absorbs and replaces the repository's former
standalone TOON-extensions document.

For an annotated, section-by-section companion to the official TOON spec and
how our implementations conform to it, see [`toon-official-spec.md`](toon-official-spec.md). For our
streaming layer, see [`toonl-reddb-spec.md`](toonl-reddb-spec.md).

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY, and OPTIONAL are to be interpreted as described in RFC 2119.

## Table of Contents

- [Baseline: the official TOON specification](#baseline-the-official-toon-specification)
- [The extension model](#the-extension-model)
  - [Enabling emission, per surface](#enabling-emission-per-surface)
- [Extension 1 — Nested tabular headers](#extension-1--nested-tabular-headers)
- [Extension 2 — Keyed-map collapse](#extension-2--keyed-map-collapse)
  - [The entry-count guardrail and its trade-off](#the-entry-count-guardrail-and-its-trade-off)
- [Extension 3 — Primitive-array columns](#extension-3--primitive-array-columns)
- [Extension 4 — Object-array columns](#extension-4--object-array-columns)
- [Extension 5 — Cyclic discriminated arrays](#extension-5--cyclic-discriminated-arrays)
- [Delimiter choice](#delimiter-choice)
- [Depth guard](#depth-guard)
- [detectTruncation — structured completeness reports](#detecttruncation--structured-completeness-reports)
- [The wire-efficiency program](#the-wire-efficiency-program)
  - [TOON vs JSON — a uniform table](#toon-vs-json--a-uniform-table)
  - [TOONL vs JSONL — at stream scale](#toonl-vs-jsonl--at-stream-scale)
- [Relationship to the streaming layer](#relationship-to-the-streaming-layer)
- [Conformance](#conformance)

## Baseline: the official TOON specification

The official TOON specification is `SPEC.md` in
[toon-format/spec](https://github.com/toon-format/spec) (**v4.1**), vendored in
this repository as the `vendor/toon-spec` git submodule. The submodule pin — the
v4.1.1 checkpoint (`vendor/toon-spec` at `62f16b3`, `vendor/toon` at `a9e6d97`)
— is the exact revision our conformance suite targets. Both implementations (the
Rust crate `reddb-io-toon` and the JS package `@reddb-io/toon`) target the full
official v4 fixture corpus at that checkpoint. Explicit expected-failure
ledgers make current coverage reproducible without turning the program target
into a blanket parity claim. v4.1
absorbed two mechanisms this repository once defined as extensions — nested
tabular headers (nested field groups, RFC spec#46) and keyed-map collapse (keyed
tabular form, RFC spec#57) — so those are now part of the baseline rather than
reddb additions.

Unless explicitly stated in this document, **TOON v4.1 governs**. This document
changes no v4.1 semantics: every valid v4.1 document decodes here with identical
meaning, and the **default encoder output is canonical TOON v4.1, byte-identical
to a spec-only implementation** — the surviving extensions below never appear in
output unless explicitly enabled.

## The extension model

The userland features have explicit, per-feature boundaries:

- **Primitive-array and object-array decoding is enabled by default.** Their
  nonstandard forms fail closed when extension handling is disabled.
- **Cyclic reconstruction is opt-in.** Without the option, its ordinary v4.1
  object wire decodes literally and preserves data without reconstructing the
  source event array.
- **Encoding is opt-in.** An encoder MUST NOT emit an extended form unless the
  caller enabled it. With no options set, output is canonical v4.1.
- **Strict behavior is explicit.** The surviving fail-closed extensions
  (primitive-array columns and object-array columns) express forms that are
  syntax errors for a spec-only v4.1 decoder, so a document using them is
  rejected rather than misread. (Nested tabular headers and keyed-map collapse
  are no longer fail-closed extensions: their forms were absorbed into the
  official spec at v4.1 and a conformant v4.1 decoder reads them natively.)
  Cyclic discriminated arrays deliberately use ordinary TOON syntax; a strict
  v4.1 decoder reads the literal grouped object and does not reconstruct the
  source array.
- **Lossless round-trip, unconditionally.** `decode(encode(x, opts)) == x` for
  every JSON value `x` and every combination of extension options. Values that do
  not fit an extension's eligibility rule fall back to standard TOON v4.1 forms.

Keeping encoding opt-in guarantees that a pipeline cannot accidentally emit a
userland wire. For primitive and object array columns, fail-closed rather than
fail-open is the safety property: a spec-only decoder errors instead of quietly
reading a different shape. The cyclic safety property is explicit literal read:
the default reader sees the grouped metadata object, not the reconstructed event
array.

### Enabling emission, per surface

| Surface | Active delimiter | Official nested/keyed forms | Primitive-array columns | Object-array columns | Cyclic discriminated arrays |
| --- | --- | --- | --- | --- | --- |
| JS — `encode(value, opts)` | `{ delimiter: ',' \| '\t' \| '\|' }` | automatic | `{ primitiveArrayColumns: true }` | `{ objectArrayColumns: true }` | `{ cyclicDiscriminatedArrays: true }` |
| Rust — `encode_with_options(value, EncodeOptions)` | `delimiter: ',' \| '\t' \| '\|'` | automatic | `primitive_array_columns: true` | `object_array_columns: true` | `cyclic_discriminated_arrays: true` |
| `tq` (TOON output) | `--delimiter comma\|tab\|pipe` | automatic; there is no flag to select them | `--primitive-array-columns` | `--object-array-columns` | `--cyclic-discriminated-arrays` |

Delimiter choice is pure TOON v4.1 for arrays and tabular rows: encoders emit
the active-delimiter declaration in the header (`[N|]`, `[N\t]`, and matching
field lists) and quote cells that contain the active delimiter. Official keyed
tabular headers carry the same declaration after the keyed marker, for example
`map[2:|]{id|name}:`.

## Extension 1 — Nested tabular headers

> **Design history — absorbed by the official spec at v4.1.** This mechanism was adopted into the official TOON specification at v4.1 (as *nested field groups*, upstream RFC [toon-format/spec#46](https://github.com/toon-format/spec/issues/46)). The official v4.1 syntax and semantics now govern; the description below is retained as design history of how the reddb flavor first shipped it, and the [proposal](proposals/nested-tabular-headers.md) records the motivation, frozen grammar, and measured numbers. Canonical encoders select the official form automatically.

The tabular form (`key[N]{fields}:`) requires every column to be a primitive.
This extension lets a column itself be a uniform nested object, declared
recursively in the header as `field{sub1,sub2}`. Rows stay flat
delimiter-separated lines; the header alone encodes the nested shape.

**Example — nested tabular headers (baseline expanded form below):**

```toon
orders[2]{id,customer{name,country},total}:
  1,Ada,UK,10.5
  2,Bob,US,20
```

decodes exactly as the baseline expanded form of:

```json
{"orders": [
  {"id": 1, "customer": {"name": "Ada", "country": "UK"}, "total": 10.5},
  {"id": 2, "customer": {"name": "Bob", "country": "US"}, "total": 20}
]}
```

**Baseline expanded form (no extension):**

```toon
orders[2]:
  - id: 1
    customer:
      name: Ada
      country: UK
    total: 10.5
  - id: 2
    customer:
      name: Bob
      country: US
    total: 20
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

> **Design history — absorbed by the official spec at v4.1.** This mechanism was adopted into the official TOON specification at v4.1 (as the *keyed tabular form*, upstream RFC [toon-format/spec#57](https://github.com/toon-format/spec/issues/57)). The official v4.1 syntax and semantics now govern; the description below is retained as design history, and the [proposal](proposals/keyed-map-collapse.md) documents the earlier no-count trade-off. Canonical encoders now select the official counted form automatically.

*Origin: upstream RFC [toon-format/spec#57](https://github.com/toon-format/spec/issues/57).*

Arrays of uniform objects get table-collapse in the baseline; keyed object *maps* with
uniform values do not, so every field name repeats once per entry. This extension
gives uniform maps the same treatment, reusing the recursive-brace header grammar
— no new sigil family:

**Example — keyed-map collapse:**

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

**Baseline expanded form (no extension):**

```toon
people:
  joe:
    first: Joe
    last: Schmoe
  mary:
    first: Mary
    last: Jane
```

**Example — single-entry map (not collapsed, below guardrail):**

```toon
config:
  timeout: 30
  retries: 3
```

```json
{"config": {"timeout": 30, "retries": 3}}
```

> **Note:** A single-entry map stays in ordinary object form even with the extension enabled, because the entry-count guardrail requires ≥2 entries for collapse eligibility.

Rules:

- The header is `key{fields}:` — object-typed because there is **no `[N]`
  segment**. Before v4.1 absorbed this form, a strict spec-only decoder rejected
  it (fail-closed) instead of reading a different shape; a conformant v4.1
  decoder now reads it natively.
- Each row is `mapKey: cells`, one line per entry, indented one level. Map keys in
  row position follow the standard TOON v4.1 key-quoting rules.
- Non-uniform maps stay in the ordinary object form. Round-trip is lossless
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
because a non-collapsed map is just standard TOON v4.1.

## Extension 3 — Primitive-array columns

> **Proposal history:** [Primitive-array columns](proposals/primitive-array-columns.md) — **stage 4 (graduated)**, landed via [#100](https://github.com/reddb-io/toon/pull/100) / [#101](https://github.com/reddb-io/toon/pull/101). The proposal carries the frozen grammar, the per-cell list-length caveat, and the measured token/byte wins.

Uniform object arrays sometimes contain fields whose values are arrays of
primitive scalars. TOON v4.1 cannot keep that containing array tabular, because
the array-valued field is not itself primitive. This extension lets an otherwise
tabular object array declare such a field as a primitive-list cell:

```toon
items[2]{id,tags[;],quantity}:
  item_0001,hazmat;oversize,60
  item_0002,oversize,11
```

This decodes to:

```json
{"items":[{"id":"item_0001","tags":["hazmat","oversize"],"quantity":60},{"id":"item_0002","tags":["oversize"],"quantity":11}]}
```

Grammar:

- In an array field header, `field[;]` declares `field` as a primitive-list cell.
- The bracket content is the in-cell sub-delimiter. The encoder currently emits
  `;`, which is valid with every active row delimiter (`comma`, `tab`, or `pipe`).
- Row cells still use the array header's active row delimiter. The list
  sub-delimiter splits only inside that one field's cell.
- Empty arrays encode as an empty cell.

Eligibility is deterministic. An encoder with the option enabled emits this form
only when **all** of the following hold:

1. the containing array is eligible for normal tabular encoding except for one or
   more primitive-list fields;
2. every primitive-list field value is an array;
3. every item in those arrays is a primitive scalar: string, number, boolean, or
   null; and
4. the list sub-delimiter differs from the active row delimiter.

Null list fields, mixed scalar/object list items, sparse rows, and heterogeneous
object shapes fall back to ordinary TOON v4.1. The encoder MUST NOT raise an
error for ordinary ineligible data.

Quoting follows the scalar cell rules. A string item is quoted when it would need
quoting as an ordinary row cell, or when it contains the list sub-delimiter. For
example:

```toon
items[1]{id,tags[;]}:
  1,"semi;quoted";plain
```

The parent `[N]` row count still checks the number of rows, and the `{fields}`
list still checks row width. The primitive-list declaration adds a type and
sub-delimiter check, but it does not declare each list cell's item count. A
malformed quoted subcell is still rejected by the quote scanner.

## Extension 4 — Object-array columns

> **Proposal history:** [Child tables + matrix](proposals/child-tables-and-matrix.md) — **stage 4 (graduated)**, landed via [#102](https://github.com/reddb-io/toon/pull/102) / [#103](https://github.com/reddb-io/toon/pull/103). The proposal covers the recursive child-table grammar and documents the fixed-width matrix form as *not recommended* for a token win.

Uniform object arrays sometimes contain fields whose values are themselves
arrays of uniform objects. TOON v4.1 must expand the parent rows because the
child array is not primitive. This extension keeps the parent table and emits
the child rows immediately below the parent row. The parent cell stores the
child row count:

```toon
orders[2|]{id|customer|items{sku|quantity|components{part|lot|ok}}}:
  ord_001|cust_a|2
    sku_1|3|2
      part_a|lot_1|true
      part_b|lot_2|false
    sku_2|1|0
  ord_002|cust_b|0
```

This decodes to the same JSON shape as the expanded TOON v4.1 list form:
`orders[].items[]` is an object array, and `items[].components[]` is another
object array. The grammar is recursive:

- `field{child,fields}` in a tabular header MAY denote either a nested object
  column or a child-table column. The row cell disambiguates the child-table
  case: it is a non-negative decimal count, and the following rows are indented
  one level deeper.
- Each child table uses the same active delimiter as the containing table.
- A child row count of `0` emits no child rows.
- The declared parent row count and every child row count are checked during
  decode; truncated child rows are an `array length mismatch`.

Eligibility is deterministic. An encoder with the option enabled emits this
form only when **all** of the following hold:

1. the containing array can otherwise be represented as a tabular object array;
2. each child-table field value is an array;
3. across all rows, every element of that child array is a non-empty object with
   the same key set; and
4. nested child-table fields satisfy the same rules recursively.

If any row contains a scalar child value, a mixed object/scalar child array, a
heterogeneous child object shape, or a depth violation, the encoder falls back
losslessly to ordinary TOON v4.1.

The same header form also carries fixed-width primitive matrices. A root or
field value shaped as a uniform non-empty list of primitive lists MAY encode as:

```toon
matrix[2|]{values[3|]}:
  1|2|3
  4|5|6
```

Here `values[3|]` declares a fixed-width list cell: each row has exactly three
primitive cells separated by the active delimiter. The single fixed-width field
decodes back to a row array, not an object wrapper.

## Extension 5 — Cyclic discriminated arrays

> **Proposal history:** [Cyclic discriminated arrays](proposals/cyclic-discriminated-arrays.md) — **stage 4 (graduated)**, landed via [#150](https://github.com/reddb-io/toon/issues/150) / [#151](https://github.com/reddb-io/toon/issues/151). The proposal records the rejected broader heterogeneous-array design, the frozen complete-cycle grammar, and the shipped benchmark re-measurement.

Strongly cyclic event streams repeat a discriminator such as `type`, `kind`, or
`event` in a stable order. TOON v4.1 repeats that discriminator in every row.
This extension emits an ordinary nested TOON object whose metadata fields carry
the original interleaving and whose tabular sub-tables group rows by
discriminator label. Scalar common-prefix fields are factored into a shared
`common[N|]{...}:` table.

For each eligible top-level array field, the emitted object MUST contain these
leading scalar fields in this order:

1. `order`: a complete-cycle expression `cycle(label[,label...])*repeats`;
2. `discriminator`: the discriminator key that was factored out of every row;
   and
3. `rows`: the original row count.

When common fields exist, the metadata fields are followed by
`common[N|]{fields}:`, where `N` equals `rows` and the table rows remain in the
original array order. After that, each discriminator label owns one tabular
sub-table. The sub-table row order is the encounter order for that label in the
original array.

```toon
events:
  order: cycle(login,purchase,logout)*2
  discriminator: type
  rows: 6
  common[6|]{tenant|seq}:
    acme|1
    acme|2
    acme|3
    acme|4
    acme|5
    acme|6
  login[2|]{actor|ok}:
    u1|true
    u2|true
  purchase[2|]{actor|amount}:
    u1|12.5
    u2|4
  logout[2|]{actor|durationMs}:
    u1|1200
    u2|900
```

This decodes to:

```json
{"events":[{"type":"login","tenant":"acme","seq":1,"actor":"u1","ok":true},{"type":"purchase","tenant":"acme","seq":2,"actor":"u1","amount":12.5},{"type":"logout","tenant":"acme","seq":3,"actor":"u1","durationMs":1200},{"type":"login","tenant":"acme","seq":4,"actor":"u2","ok":true},{"type":"purchase","tenant":"acme","seq":5,"actor":"u2","amount":4},{"type":"logout","tenant":"acme","seq":6,"actor":"u2","durationMs":900}]}
```

Grammar:

- `order` MUST use the complete-cycle RLE grammar
  `cycle(label[,label...])*repeats`. Tail forms are invalid. The expanded order
  length MUST equal `rows`.
- `discriminator` MUST be a scalar key. The key MUST NOT appear in the common
  table header or any discriminator sub-table header, because decode restores it
  from the sub-table label selected by `order`.
- `rows` MUST be a non-negative integer equal to the expanded `order` length.
- `common[N|]{...}:` is OPTIONAL. When present, `N` MUST equal `rows`; each
  row MUST have exactly the declared leaf width; each header leaf MUST be a
  scalar common field; and each row supplies the common values for the matching
  original array row.
- Each discriminator label appearing in `order` MUST have exactly one sub-table
  named by that label. The sub-table row count MUST equal the number of
  occurrences of the label in the expanded order.
- The decoder consumes one row from the matching label sub-table each time that
  label appears in `order`, combines it with the same-index common row when a
  common table is present, and MUST consume every declared sub-table row exactly
  once.
- Nested object payloads use dotted-path flattening in sub-table headers, such
  as `issue.number|issue.title`. Array-valued payloads are flattenable only when
  the array shape is uniform for every row in that discriminator sub-table; the
  header MUST include a `.length` guard column plus fixed numeric element paths
  such as `labels.length|labels.0|labels.1`. Non-uniform nested object shapes,
  non-uniform array lengths, nested arrays, and mixed scalar/object paths are
  ineligible for this extension and fall back to canonical TOON v4.1.

With cyclic reconstruction enabled, a malformed cyclic object,
missing metadata field, duplicate metadata field, invalid order expression,
wrong common-row width, common-row count mismatch, missing discriminator
sub-table, duplicate discriminator sub-table, sub-table length mismatch, or
non-uniform flattened nested shape MUST be rejected. The decoder reconstructs
each row as:

```js
{ [discriminator]: label, ...commonRow, ...groupPayload }
```

A strict TOON v4.1 decoder has no knowledge of this extension and therefore
MUST read the wire literally as the nested object shown above: scalar `order`,
`discriminator`, and `rows` fields plus ordinary tabular fields named `common`
and by discriminator labels. Strict v4.1 read is not an error and does not
reconstruct the original array. Extension-aware decoders are required when the
wire is intended to round-trip back to the source array.

Encoder eligibility is deterministic. An encoder with the option enabled emits
this form only when **all** of the following hold:

1. the root value is a non-empty object whose fields are unique and whose values
   are arrays;
2. every item in every candidate array is an object with unique keys;
3. every row has the same scalar string discriminator key, chosen in priority
   order from `type`, `kind`, then `event`;
4. the discriminator key, root keys, and common field keys contain no whitespace,
   comma, or equals sign;
5. the discriminator sequence is a complete repeated cycle of unique labels,
   with cycle length 2 through 8 and at least three full repeats;
6. the compact order expression is at most 40% of the raw percent-encoded
   per-row discriminator list; and
7. common fields, if any, are the contiguous primitive-valued fields immediately
   after the discriminator in the first row and present as primitive values in
   every row; and
8. every discriminator-specific payload sub-table has a uniform scalar leaf
   shape after dotted-path flattening. Nested objects are eligible only when all
   rows for that discriminator share the same nested object shape; arrays are
   eligible only when they are primitive arrays with the same fixed length across
   all rows in that discriminator sub-table.

Any ordinary ineligible value falls back to canonical TOON v4.1. The encoder
MUST NOT raise an error merely because a value is irregular, too short,
partially cyclic, random, has non-string discriminator values, lacks an eligible
discriminator, contains a tail after the last complete cycle, or fails the order
compression threshold.

The shipped benchmark re-measurement in
[`benchmarks/results/2026-08-06-token-efficiency.md`](../benchmarks/results/2026-08-06-token-efficiency.md)
measured the implemented tabular wire through both shipped implementations. On
the representative cyclic shape, the best TOON-family format was the cyclic
extension with a median **26.8% token reduction versus minified JSON**. The
amortization curve crossed over at 24 records and measured 20.9%, 26.7%, 27.2%,
and 26.9% token reductions for the 24-, 90-, 240-, and 500-row cyclic datasets.

## Delimiter choice

> **Proposal history:** [Delimiter choice](proposals/delimiter-choice.md) — **stage 4 (graduated)**. The proposal records why comma stays the default and when to reach for tab or pipe, with measured trade-offs and the upstream RFC link.

TOON v4.1 supports three delimiters — comma (default), tab (HTAB), and pipe
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

**Example — comma-delimited (default):**

```toon
items[2]{id,description}:
  1,pen
  2,"eraser, pink"
```

```json
{"items": [{"id": 1, "description": "pen"}, {"id": 2, "description": "eraser, pink"}]}
```

**Example — tab-delimited (cells with commas unquoted):**

```toon
data[2	]{value	note}:
  100	item, qty 5
  200	item, qty 3
```

```json
{"data": [{"value": 100, "note": "item, qty 5"}, {"value": 200, "note": "item, qty 3"}]}
```

**Example — pipe-delimited (human-readable tables):**

```toon
users[2|]{name|status}:
  Alice|active
  Bob|inactive
```

```json
{"users": [{"name": "Alice", "status": "active"}, {"name": "Bob", "status": "inactive"}]}
```

**Example — nested headers with different delimiters:**

Nested comma-delimited:

```toon
items[1]{id,config}:
  1,"a,b"
```

```json
{"items": [{"id": 1, "config": "a,b"}]}
```

The flavor keeps the spec's rule that **absence of a delimiter symbol always means
comma**, with no inheritance from a parent header, so a nested header's delimiter
is always locally legible. Delimiter selection never changes the decoded value;
it is purely a wire-efficiency and readability lever, and the round-trip is
lossless for every choice.

## Depth guard

> **Proposal history:** [Depth guard](proposals/depth-guard.md) — **stage 4 (graduated)**. The proposal explains the stack-exhaustion threat this closes, the `1000` default, and why it never changes a decoded value.

Neither the official spec's data model nor its strict-mode checklist bounds
nesting depth; a maliciously or accidentally deep document can drive a naïve
recursive decoder into stack exhaustion. The reddb-io flavor adds a **depth
guard** as a robustness measure that does not change any decoded value.

- Decoding is bounded by `DecodeOptions::max_depth` (Rust) and the equivalent JS
  parse option; checked encoding is bounded by `EncodeOptions::max_depth`.
- **Both default to `1000`.** A document nested deeper than the guard is rejected
  with a structured error rather than crashing the process.
- Setting `max_depth` to `0` disables the guard and MUST be done **only for
  trusted input**.
- On the encode side, prefer the checked entry points
  (`try_to_canonical_toon()` / `try_to_toon_with_options(...)` in Rust) when
  encoding untrusted or user-supplied values, so a depth failure returns an
  `EncodeError` instead of overflowing.

**Example — deeply nested structure (within default limit of 1000):**

```toon
a:
  b:
    c:
      value: 42
```

```json
{"a": {"b": {"c": {"value": 42}}}}
```

**Example — depth violation (default max_depth=1000 exceeded):**

A pathologically deep document nesting 1001+ levels is rejected:

```
error: maximum nesting depth (1000) exceeded
```

The guard is a defense-in-depth default, not a format change: a document within
the limit decodes identically whether or not the guard is present, and the limit
is configurable for callers whose inputs are known-shallow or known-trusted.

## detectTruncation — structured completeness reports

> **Proposal history:** [detectTruncation](proposals/detect-truncation.md) — **stage 4 (graduated)**. The proposal shows how TOON's self-checking guardrails become a first-class diagnostic API across all three surfaces, with the stable report schema.

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
The canonical reproducible evidence lives in `benchmarks/`: run
`pnpm benchmark:tokens` for deterministic bytes and `o200k_base` token counts,
and read dated reports in `benchmarks/results/`. This spec intentionally avoids
embedding benchmark result tables so the normative grammar does not drift from
the measured reports.

## Relationship to the streaming layer

TOONL ([`toonl-reddb-spec.md`](toonl-reddb-spec.md)) is an independent line-oriented streaming
extension with its own versioning; it is unaffected by this document. The
TOONL close-transform continues to target canonical TOON v4.1 documents and does
**not** emit the primitive-array-column, object-array-column, or cyclic-
discriminated-array extension forms defined here. It may emit nested field
groups and keyed tabular form because those are canonical v4.1. The two
concerns compose cleanly but are specified separately.

## Conformance

The shared corpora under `tests/` pin both implementations to the same expected
behavior. Reproduce the public paths with
`node --test packages/toon/test/conformance.test.mjs` and
`cargo test -p reddb-io-toon --test spec_conformance`:

- `vendor/toon-spec/tests/fixtures/` — the pinned upstream v4.1
  baseline, run by both the Rust crate and the JS package.
- `tests/corpus/toon/` and `tests/corpus/wire-efficiency/` — exact encode bytes
  and decode values for canonical forms and local extensions, including
  eligibility and fail-closed cases.
- `tests/corpus/json-limits.json` — the shared JSON edge corpus (numbers at the
  boundaries of the safe-integer range, precision, and other parser limits) run
  identically by the JS package and the Rust crate.
- The `tq` golden tests cover the extension emission flags end-to-end, including
  `--object-array-columns` and `--cyclic-discriminated-arrays`.

The workspace commands `pnpm -r test` and `cargo test --workspace` run the
broader package/crate checks. Any conformance claim in this document is scoped
to those commands, the pinned fixtures, and the checked-in ledgers.
