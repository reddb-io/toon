# TOON v3.3 — Annotated Specification & Implementation Companion

**tl;dr.** This document annotates the official TOON v3.3 specification with implementation notes, corner cases, and executable examples for the two reddb-io runtimes (Rust and JavaScript). We are grateful to the [toon-format](https://github.com/toon-format/spec) team and author Johann Schopplich for the clean, deterministic format that enables 100% conformance across independent implementations.

## Acknowledgment

This document is an *annotated companion* to the official **TOON** specification
— the Token-Oriented Object Notation created and stewarded by the
[toon-format](https://github.com/toon-format/spec) team and its author, Johann
Schopplich. Every design decision walked through below is theirs; our
contribution is only the annotation — the corner cases we hit, the decision trees
we followed, and the record of how our two implementations conform. We are
grateful for a specification written with the rigor of an RFC and the restraint
of a good format: explicit lengths, deterministic quoting, one active delimiter,
indentation instead of braces. It is because the base is so clean that we can
claim 100% conformance in two runtimes and mean it.

**Source and attribution.** The normative text this companion follows is
`SPEC.md` from [toon-format/spec](https://github.com/toon-format/spec), **Working
Draft v3.3** (dated 2026-05-21, author Johann Schopplich), released under the
**MIT License**. It is vendored in this repository as the `vendor/toon-spec` git
submodule, pinned at commit
**`f55b93ac489f297ff597d95e4c19ae84675eaeb7`**. That pin is the exact revision our
conformance suite runs against. The MIT License permits derivation with
attribution; this document derives structure and quotes short normative fragments
from the official spec and attributes them to that source and pin. For the
authoritative text, always read `vendor/toon-spec/SPEC.md` at the pinned commit.
This companion is informative about our implementation and does not override the
official specification.

The key words MUST, MUST NOT, SHOULD, MAY, etc. carry their RFC 2119 meaning when
quoted from the official spec; our implementation notes use them descriptively.

## How to read this companion

Each section below mirrors a section of the official v3.3 spec, in order. Within
each we give:

- **What it defends** — the intent of the official section and its main decision
  tree.
- **Corner cases** — the edges the spec calls out (or that bite implementers).
- **Our implementation** — how the Rust crate (`reddb-io-toon`) and the JS package
  (`@reddb-io/toon`) handle it, and where a shared corpus pins the behavior.

Our *engineering wins* — dual-runner corpora, the json-limits corpus, the depth
guard, and registry-verified releases — are woven in where the relevant section
motivates them, and summarized under [Implementation guarantees](#implementation-guarantees).

Two things this companion is **not**: it is not the reddb-io *extensions* (nested
tabular headers, keyed-map collapse, the depth-guard rationale, wire-efficiency
numbers) — those live in
[`toon-spec-reddb-flavored.md`](toon-spec-reddb-flavored.md) — and it is not the
streaming layer, which is [`toonl.md`](toonl.md).

## Table of Contents

- [Abstract & Status](#abstract--status-official-§abstract-§status)
- [§1 Terminology and Conventions](#§1-terminology-and-conventions)
- [§2 Data Model](#§2-data-model)
- [§3 Encoding Normalization](#§3-encoding-normalization-reference-encoder)
- [§4 Decoding Interpretation](#§4-decoding-interpretation-reference-decoder)
- [§5 Concrete Syntax and Root Form](#§5-concrete-syntax-and-root-form)
- [§6 Header Syntax](#§6-header-syntax-normative)
- [§7 Strings and Keys](#§7-strings-and-keys)
  - [§7.1 Escaping](#§71-escaping)
  - [§7.2 Quoting Rules for String Values](#§72-quoting-rules-for-string-values)
  - [§7.3 Key Encoding](#§73-key-encoding)
  - [§7.4 Decoding Rules for Strings and Keys](#§74-decoding-rules-for-strings-and-keys)
- [§8 Objects](#§8-objects)
- [§9 Arrays](#§9-arrays)
  - [§9.1 Primitive Arrays (Inline)](#§91-primitive-arrays-inline)
  - [§9.2 Arrays of Arrays](#§92-arrays-of-arrays-primitives-only)
  - [§9.3 Arrays of Objects — Tabular Form](#§93-arrays-of-objects--tabular-form)
  - [§9.4 Mixed / Non-Uniform Arrays — Expanded List](#§94-mixed--non-uniform-arrays--expanded-list)
- [§10 Objects as List Items](#§10-objects-as-list-items)
- [§11 Delimiters](#§11-delimiters)
- [§12 Indentation and Whitespace](#§12-indentation-and-whitespace)
- [§13 Conformance and Options](#§13-conformance-and-options)
  - [§13.4 Key Folding and Path Expansion](#§134-key-folding-and-path-expansion)
- [§14 Strict Mode Errors](#§14-strict-mode-errors-authoritative-checklist)
- [§15 Security Considerations](#§15-security-considerations)
- [§16 Internationalization](#§16-internationalization)
- [§17 IANA Considerations](#§17-iana-considerations)
- [§18 Versioning and Extensibility](#§18-versioning-and-extensibility)
- [§19 Intellectual Property Considerations](#§19-intellectual-property-considerations)
- [Appendices](#appendices-a–f-informative)
- [Implementation guarantees](#implementation-guarantees)

## Abstract & Status (official §Abstract, §Status)

**What it defends.** TOON is a line-oriented, indentation-based text format that
encodes the JSON data model with explicit structure and minimal quoting. Arrays
declare length and an optional field list once; rows use a single active delimiter
(comma, tab, or pipe). It is a Working Draft — stable for implementation but not
finalized; breaking changes may occur across major versions.

**Our implementation.** We treat the pinned submodule as the contract and track
upstream by advancing the pin deliberately, never by drifting a vendored copy.
Because the status is "Working Draft", our conformance suite reads fixtures **live
from the submodule**, so an upstream change surfaces as a test signal rather than
silent divergence.

## §1 Terminology and Conventions

**What it defends.** A shared vocabulary and the RFC 2119 normativity rule:
keywords are normative only in all-caps, and all normative text lives in §§1–16
(appendices are informative unless marked otherwise). It fixes the core structural
terms (indentation *level*/depth, indentation *unit*/`indentSize`), the array
terms (header, field list, list item), and — critically — the **three delimiter
roles**: the delimiter character, the *document delimiter* (quoting decisions
outside any array scope), and the *active delimiter* (declared by the nearest
array header).

**Decision tree it sets up.** The document-vs-active delimiter split (§1.5) is the
root of most quoting decisions later: object field values quote against the
document delimiter; inline array values and tabular cells quote against the active
delimiter. §1.9 introduces the stricter `IdentifierSegment`
(`^[A-Za-z_][A-Za-z0-9_]*$`) used only for key-folding/expansion eligibility,
distinct from the permissive unquoted-key pattern (which allows dots).

**Corner cases.** The `IdentifierSegment` vs unquoted-key distinction is a
frequent implementer trap: dotted keys are *valid literal keys* but *not*
fold/expand-eligible segments. Tabs are legal as a delimiter and inside quoted
strings but never as indentation.

**Our implementation.** Both runtimes model the two delimiter roles explicitly and
keep the folding-eligibility predicate separate from the unquoted-key predicate,
so a dotted literal key is never accidentally expanded (see §13.4).

## §2 Data Model

**What it defends.** TOON carries exactly the JSON data model —
primitive/object/array — and pins two ordering guarantees (array order preserved;
object key order preserved as encountered) and a **canonical number form**.

**The number decision tree (encoding).** For `n = 0` or `1e-6 ≤ |n| < 1e21`:
canonical decimal, *no* exponent, no leading zeros (except the single `0`), no
trailing fractional zeros, integer-valued numbers emitted as integers, and `-0`
normalized to `0`. Outside that range (non-zero `|n| < 1e-6`, or `|n| ≥ 1e21`),
encoders MAY use JSON exponent notation, SHOULD use lowercase `e` with an explicit
sign for determinism. The overriding requirement: emit enough precision that
`decode(encode(x)) == x` under JSON-model equality.

**Example — canonical number forms:**

```toon
values: 0,1,1000000,1.5,0.000001,0
```

```json
{"values": [0, 1, 1000000, 1.5, 0.000001, 0]}
```

> **Note:** `1e6` encodes as `1000000` (no exponent), `1.0` as `1` (integer form), `-0` as `0` (normalized).

**Corner cases.** `1e6` MUST render as `1000000`; `1.0` as `1`; `1.5000` as `1.5`.
Out-of-domain numbers (arbitrary-precision decimals, integers beyond the host
domain) may be stringified losslessly *or* approximated, but the choice MUST be
documented, and encoders SHOULD expose lossless stringification.

**Our implementation & engineering win — the json-limits corpus.** Number handling
is exactly where two independent runtimes drift, so we pin it with a **shared
JSON-edge corpus**, `tests/json-limits/corpus.json` (`json-limits-v0.1`), run
identically by the JS package and the Rust crate. It fixes behavior at the
boundaries — `9007199254740991` (max safe integer), `…992` (one past it),
precision-sensitive decimals — asserting the same canonical TOON string and the
same round-trip JSON from both. This is how we keep the crate's `i64`/`f64` domain
and JavaScript's IEEE-754 `number` from disagreeing about a boundary value.

## §3 Encoding Normalization (Reference Encoder)

**What it defends.** Non-JSON host values MUST be normalized to the JSON model
before encoding, and the mapping MUST be documented. `NaN`/`±Infinity → null`.
Implementations MAY honor host serialization hooks (`toJSON()`,
`serde::Serialize`, `json.Marshaler`, …), which SHOULD take precedence and MUST be
documented. Informative mappings: dates → ISO 8601 strings, sets → arrays, maps →
objects, unrecognized host values → null.

**Our implementation.** The JS package normalizes via the standard JS value model;
the Rust crate normalizes from its `Value` model and from `serde` inputs. Both
document their host-type mapping; `NaN`/`±Infinity → null` is enforced on the
encode path.

## §4 Decoding Interpretation (Reference Decoder)

**What it defends.** How text tokens become host values. Quoted tokens are strings
(unescaped per §7.1) even if they look numeric; unquoted `true`/`false`/`null`
decode to boolean/null; numeric parsing accepts decimal and exponent input.

**The leading-zero decision tree.** Tokens with forbidden leading zeros in the
*integer* part (`05`, `0001`, `-05`) decode as **strings**, not numbers — but a
single `0` integer part followed by a fraction or exponent (`0.5`, `0e1`, `-0.5`)
is a valid number. `-0 → 0`. The literal `[]` in field or root position decodes as
an empty array. Everything else unquoted → string. A key MUST be followed by a
colon.

**Corner cases.** `1.5000 → 1.5`, `-1E+03 → -1000`. Out-of-domain decoded numbers
MAY be widened, stringified, or approximated per a documented policy;
lossless-first is RECOMMENDED for interchange libraries.

**Our implementation.** Both runtimes implement the leading-zero rule exactly (it
is a common source of silent wrong answers) and document their out-of-range
policy. The json-limits corpus (§2) covers the boundary tokens on the decode side
too — a decode "passes" only when the value equals the fixture's expected JSON
*and* our own canonical output round-trips to it (see [Implementation
guarantees](#implementation-guarantees)).

## §5 Concrete Syntax and Root Form

**What it defends.** The line/indentation model and, importantly, the **root-form
discovery** decision tree.

**Root-form decision tree.** In order:
1. First non-empty depth-0 line is a valid root array header → **root array**.
2. Exactly one non-empty line, the literal `[]` → **empty root array**.
3. Exactly one non-empty line, neither a header nor a key-value line →
   **single primitive** (`hello`, `42`, `true`).
4. Otherwise → **object**.
5. Empty document (no non-empty lines) → **empty object `{}`**.

**Examples — root forms:**

Single primitive:

```toon
42
```

```json
42
```

Empty root array:

```toon
[]
```

```json
[]
```

Root object:

```toon
name: Alice
age: 30
```

```json
{"name": "Alice", "age": 30}
```

Root array:

```toon
[2]{id,name}:
  1,Alice
  2,Bob
```

```json
[{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]
```

**Corner cases.** In strict mode, two-or-more non-empty depth-0 lines that are
neither headers nor key-value lines is invalid (e.g. `hello\nworld` is *not* two
primitives — it is a malformed document).

**Example — invalid root (two bare primitives):**

```toon
hello
world
```

**Expected error:** malformed document; depth-0 lines must be a header, key-value, or single primitive.

**Our implementation.** Both runtimes implement this exact precedence; the
"two bare primitives at root" rejection is covered by the shared corpus.

## §6 Header Syntax (Normative)

**What it defends.** Array headers declare length, active delimiter, and optional
field names, with a normative ABNF. Forms: `[N]:`, `key[N]:`,
`key[N]{f1,f2}:`; delimiter symbol absent = comma, HTAB = tab, `|` = pipe.

**Examples — header forms:**

Inline primitive array:

```toon
items[3]: apple,banana,cherry
```

```json
{"items": ["apple", "banana", "cherry"]}
```

Tabular array (field-list required):

```toon
users[2]{id,name}:
  1,Alice
  2,Bob
```

```json
{"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]}
```

Tab-delimited tabular array:

```toon
data[2]	id	value:
	1	100
	2	200
```

```json
{"data": [{"id": 1, "value": 100}, {"id": 2, "value": 200}]}
```

**The rules that bite.**
- Length MUST be a non-negative integer with **no leading zeros** (`0` is the only
  canonical zero); `[03]`, `[-1]`, `[bar]` are not bracket segments.
- **No content between `]` and `{`/`:`** — `[1][bar]:`, `[2]extra:`, `[2] :` are
  strict-mode errors; non-strict decoders MAY fall through to key-value parsing
  with the key as a literal token.
- The bracket/fields delimiter-equality rule is *not* expressed by the ABNF;
  implementations enforce it, and a mismatch MUST error in strict mode.
- Absence of a delimiter symbol **always** means comma, with no inheritance from a
  parent header.
- Folded key prefixes (§13.4) are allowed as the key part; the bracket/field
  grammar is unchanged.

**Example — invalid header (leading zero in length):**

```toon
items[03]: a,b,c
```

**Expected error:** malformed bracket segment; `03` has a leading zero.

**Our implementation.** Header parsing follows Appendix B.2: isolate the optional
key prefix (quoted literal or up-to-first-`[`), parse length, then the optional
trailing delimiter symbol, then the optional `{…}` fields, then require the colon.
Delimiter equality between bracket and fields is enforced, not merely parsed.

## §7 Strings and Keys

### §7.1 Escaping

**What it defends.** A closed escape repertoire (`\\`, `\"`, `\n`, `\r`, `\t`,
`\uXXXX` for other C0 controls) with a **first-matching-row** table and separate
encoder/decoder columns. Lone surrogates (`U+D800–U+DFFF`) MUST be rejected when
decoded from `\uXXXX`; supplementary scalars MUST be emitted as literal UTF-8, and
surrogate escapes for them MUST be rejected. Decoders MUST reject any unlisted
escape, `\u` with fewer than four hex digits, and unterminated strings.

**Corner cases.** Tabs are `\t` inside quoted strings on encode, though the ABNF's
`unescaped-char` tolerates a literal HTAB on decode. Hex in `\uXXXX` is
case-insensitive on decode, lowercase SHOULD on encode.

**Our implementation.** Both runtimes implement the exact table and the surrogate
rejections; the "reject invalid escape / unterminated string" cases are in the
shared corpus (e.g. `name: "bad\xescape"` from Appendix A error cases).

### §7.2 Quoting Rules for String Values

**The quote-or-not decision tree.** A string MUST be quoted if any hold: empty;
leading/trailing whitespace; equals `true`/`false`/`null`; numeric-like (matches
`/^-?\d+(?:\.\d+)?(?:e[+-]?\d+)?$/i`); contains `:`, `"`, `\`, brackets or braces;
contains a C0 control; contains the *relevant* delimiter (active for
array/tabular cells, document for object field values); equals `-` or starts with
`-`. Otherwise it MAY be unquoted — Unicode, emoji, and internal spaces are safe.

**Examples — quoting rules:**

Unquoted strings (safe):

```toon
simple: hello
with_internal_spaces: "hello world"
emoji: 🎉
```

```json
{"simple": "hello", "with_internal_spaces": "hello world", "emoji": "🎉"}
```

Quoted strings (must be quoted):

```toon
empty: ""
numeric_string: "05"
boolean_like: "true"
has_colon: "http://example.com"
```

```json
{"empty": "", "numeric_string": "05", "boolean_like": "true", "has_colon": "http://example.com"}
```

**Corner cases.** The "relevant delimiter" is context-dependent (§11): the same
string may need quoting as a tabular cell but not as an object field value under a
different document delimiter. `"05"` is numeric-like → quoted so it survives as a
string.

**Our implementation.** Both runtimes compute the relevant delimiter from context
(active vs document) before applying the predicate — the single most important
detail for byte-identical output between the two runtimes.

### §7.3 Key Encoding

**What it defends.** Keys and field names MAY be unquoted only if they match
`^[A-Za-z_][A-Za-z0-9_.]*$`; otherwise MUST be quoted and escaped. Keys requiring
quoting MUST be quoted in *all* contexts, including array headers (`"my-key"[N]:`).

**Our implementation.** Enforced uniformly across object fields, tabular field
names, and header key prefixes; the `"my-key"[3]:` and `"x-items"[2]{…}:` cases
from Appendix A are in the corpus.

### §7.4 Decoding Rules for Strings and Keys

Quoted keys MUST be unescaped per §7.1; a key MUST be followed by `:` or the
decoder MUST error. Handled identically in both runtimes.

## §8 Objects

**What it defends.** `key: value` for primitives (single space after colon);
`key:` alone opens a nested/empty object; key order preserved on emit; an empty
root object yields an empty document.

**Examples — objects:**

Flat object:

```toon
user: Alice
status: active
age: 30
```

```json
{"user": "Alice", "status": "active", "age": 30}
```

Nested object (bare `key:`):

```toon
config:
  timeout: 30
  retries: 3
```

```json
{"config": {"timeout": 30, "retries": 3}}
```

Explicit empty array vs empty object:

```toon
items: []
metadata:
```

```json
{"items": [], "metadata": {}}
```

**The bare-`key:` decision.** A bare `key:` with nothing after the colon MUST
decode as an empty/nested **object**, *not* an empty array — empty arrays use the
explicit `key: []` form (§9.1). Dotted keys are single literal keys unless path
expansion is enabled (§13.4). Duplicate sibling keys → §14.4.

**Corner cases.** The object-vs-empty-array distinction at `key:` is a classic
ambiguity the spec resolves firmly in favor of object; getting it wrong silently
changes shape.

**Our implementation.** Both runtimes decode bare `key:` to an object and require
`key: []` for the empty array, matching the spec exactly.

## §9 Arrays

### §9.1 Primitive Arrays (Inline)

`key[N]: v1,v2,…`, split on the active delimiter; empty tokens (even
whitespace-surrounded) decode to the empty string; strict mode requires the
decoded count to equal `N`. Empty arrays: `key: []` / `[]` preferred, legacy
`key[0]:` / `[0]:` accepted.

**Example — inline primitive array:**

```toon
scores[3]: 95,87,92
```

```json
{"scores": [95, 87, 92]}
```

**Example — primitive array with empty tokens:**

```toon
values[4]: 1,,3,
```

```json
{"values": [1, "", 3, ""]}
```

### §9.2 Arrays of Arrays (Primitives Only)

Parent header `key[N]:` then `- [M]: …` list items at depth +1; inner arrays split
on their own active delimiter; strict mode enforces both `M` and outer `N`. The
`key: []` field-form does **not** apply to list-item inner arrays.

**Example — array of arrays:**

```toon
matrix[2]:
  - [2]: 1,2
  - [2]: 3,4
```

```json
{"matrix": [[1, 2], [3, 4]]}
```

### §9.3 Arrays of Objects — Tabular Form

**The tabular-detection decision tree (encoding, MUST hold for all elements):**
every element is an object; each has ≥1 key; **no** element is an empty `{}`; all
share the same key set (per-object order MAY vary); all values are primitives.
When satisfied, emit `key[N]{fields}:` with field order from the first object's
key encounter order, one row per object at depth +1.

**Example — tabular array:**

```toon
users[2]{id,name,active}:
  1,Alice,true
  2,Bob,false
```

```json
{"users": [{"id": 1, "name": "Alice", "active": true}, {"id": 2, "name": "Bob", "active": false}]}
```

**Example — tabular array with quoted colon in cell:**

```toon
links[2]{title,url}:
  "Example Inc","http://example.com"
  "Another","http://a.co"
```

```json
{"links": [{"title": "Example Inc", "url": "http://example.com"}, {"title": "Another", "url": "http://a.co"}]}
```

**The row-vs-key-value disambiguation (decoding).** At row depth, for unquoted
tokens: compute the first unquoted active delimiter and the first unquoted colon.
No unquoted colon → **row**. Both present: delimiter-before-colon → **row**,
colon-before-delimiter → **key-value line (rows end)**. Colon but no delimiter →
**key-value line**. Strict mode enforces row width = field count and row count =
`N`.

**Corner cases.** A quoted colon inside a cell (`1,"http://a:b"`) does not end the
rows — only *unquoted* positions count. Tabular arrays as the first field of a
list-item object interact with §10 indentation.

### §9.4 Mixed / Non-Uniform Arrays — Expanded List

When tabular requirements fail: `key[N]:` then one list item per element —
`- <primitive>`, `- [M]: …` for primitive arrays, nested headers for
arrays-of-objects/non-uniform (tabular form is unavailable in this nested
position; expanded list MUST be used), objects per §10. Strict mode enforces list
count = `N`.

**Example — expanded list (mixed elements):**

```toon
items[3]:
  - apple
  - [2]: 1,2
  - nested:
      key: value
```

```json
{"items": ["apple", [1, 2], {"nested": {"key": "value"}}]}
```

**Our implementation.** Both runtimes implement the first-unquoted-position
comparison verbatim; the quoted-colon and mixed cases are pinned by the corpus.

### §9.4 Mixed / Non-Uniform Arrays — Expanded List

When tabular requirements fail: `key[N]:` then one list item per element —
`- <primitive>`, `- [M]: …` for primitive arrays, nested headers for
arrays-of-objects/non-uniform (tabular form is unavailable in this nested
position; expanded list MUST be used), objects per §10. Strict mode enforces list
count = `N`.

## §10 Objects as List Items

**What it defends.** How an object renders as a list item, and specifically the
**tabular-first-field** case. Empty object list item is a bare `-`. When a
list-item object's *first field in encounter order* is a tabular array, encoders
MUST put the tabular header on the hyphen line
(`- key[N]{fields}:`), rows at depth +2, and all other fields at depth +1 — never
rows at +1 or sibling fields level with rows. For all other cases the first field
SHOULD go on the hyphen line.

**Example — list item with tabular first field (depth +2 rows, +1 siblings):**

```toon
orders[2]:
  - items[2]{id,qty}:
      1,5
      2,3
    status: pending
  - items[1]{id,qty}:
      3,1
    status: shipped
```

```json
{"orders": [{"items": [{"id": 1, "qty": 5}, {"id": 2, "qty": 3}], "status": "pending"}, {"items": [{"id": 3, "qty": 1}], "status": "shipped"}]}
```

**Corner cases.** This is the fiddliest indentation rule in the spec (depth +2 for
rows, +1 for sibling fields, relative to the hyphen). The decoder mirror: a
`- key[N]{fields}:` line starts a tabular field; depth +2 lines are its rows; a
depth +1 line after rows terminates them.

**Our implementation.** Both runtimes implement the +2/+1 relative-indentation rule
and its decode mirror; Appendix A's "nested tabular inside a list item" example is
in the corpus.

## §11 Delimiters

**What it defends.** The three delimiters and, in §11.1/§11.2, the encode/decode
consequences of the document-vs-active split: inline array values and tabular
cells quote/split against the **active** delimiter (nearest header); object field
values quote against the **document** delimiter regardless of enclosing array
scope; on decode, object field values are parsed as a single post-colon token (the
document delimiter is not a decoder concept). Nested headers may change the active
delimiter; empty tokens are preserved with surrounding spaces trimmed.

**Our implementation.** The active/document split is a first-class concept in both
runtimes' quoting and splitting paths — see §7.2. This is the reddb *flavor's*
lever for tab/pipe delimiter choice, documented in
[`toon-spec-reddb-flavored.md`](toon-spec-reddb-flavored.md#delimiter-choice);
the base behavior here is pure v3.3.

## §12 Indentation and Whitespace

**What it defends.** Encoders MUST use a consistent spaces-per-level (default 2, no
tabs), exactly one space after `key:` and after array headers with inline values,
no trailing spaces, and **no trailing newline**. Strict decoders require
leading-space counts to be exact multiples of `indentSize` and reject tab
indentation; non-strict decoders MAY use `floor(spaces/indentSize)` and MAY accept
tabs (policy documented). Blank lines are ignored outside arrays but are strict
errors inside arrays/tabular rows.

**Corner cases.** The "no trailing newline at EOF" encode rule vs "decoders SHOULD
accept a trailing newline" decode rule is an asymmetry worth internalizing:
canonical output has none, but decode tolerates one.

**Our implementation.** Canonical output from both runtimes emits no trailing
newline and no trailing spaces; strict decode enforces the indentation-multiple
and no-tab-indent rules. The whitespace invariants are part of what the encode
corpus checks byte-for-byte.

## §13 Conformance and Options

**What it defends.** Per-class checklists (§13.1 encoder, §13.2 decoder, §13.3
validator) and the option set: encoder `indentSize`, `delimiter`, `keyFolding`,
`flattenDepth`; decoder `indentSize`, `strict` (default true), `expandPaths`.
Option names are concept handles; host-idiomatic spellings are allowed if
documented.

**Our implementation.** Both runtimes satisfy every applicable checklist item at
the pinned corpus. `strict` defaults to true; the decoder options
(`indent`/`indentSize`, `strict`, `expandPaths`) are exposed on both surfaces
(`parse(input, options)` in JS, `ParseOptions`/`Document::parse_with_options` in
Rust). Note our **depth guard** (`max_depth`, default 1000) is an *additional*
robustness option beyond the spec's set — it changes no decoded value and is
documented in the flavored spec.

### §13.4 Key Folding and Path Expansion

**What it defends.** Optional, symmetric transforms (both default `"off"`):
encoder *key folding* collapses chains of single-key objects into dotted paths;
decoder *path expansion* splits dotted keys back into nesting.

**The foldability decision tree.** A chain `K1→…→KL` is foldable when each
non-terminal `Ki` is an object with exactly one key, the chain stops at the first
non-single-key object / leaf / array, and the leaf is a primitive, array, or empty
object. **Safe-mode** requires every folded segment to be an `IdentifierSegment`
(§1.9, no dots) and the folded key to not collide with an existing sibling literal
key. `flattenDepth` bounds how many segments fold (default Infinity; `<2` has no
effect).

**The expansion conflict decision tree.** Object+Object → deep merge;
Object+Non-object, Array+anything, Primitive+Primitive → **conflict**. With
`strict=true` (default) a conflict MUST error; with `strict=false`, last-write-wins
in document order, silently. Expansion runs *after* base parsing (§§4–12) and
*before* returning the value; §14 structural checks operate on the pre-expanded
structure.

**Corner cases.** `{data: {"full-name": {x:1}}}` is *not* folded (`"full-name"` is
not an `IdentifierSegment`). `a.b: 1` then `a: 2` errors under strict expand,
LWW-resolves to `{a:2}` under non-strict. Quoted dotted keys stay literal.

**Our implementation.** Both runtimes gate folding/expansion on the
`IdentifierSegment` predicate (kept separate from the permissive unquoted-key
predicate, §1) and implement the strict-conflict/LWW branch per §14.3. Both default
`keyFolding`/`expandPaths` to `"off"`, so round-trip is literal unless the caller
opts in.

## §14 Strict Mode Errors (Authoritative Checklist)

**What it defends.** The authoritative list of strict-mode rejections, grouped:
§14.1 array count/width mismatches (inline count ≠ `N`, list items ≠ `N`, rows ≠
`N`, row width ≠ field count — count checks apply only when an explicit `[N]` is
declared, so `key: []` is N/A); §14.2 syntax/structural errors (missing colon,
invalid escape / unterminated string, header delimiter mismatch, malformed bracket
lengths, content between bracket and colon, indentation/blank-line invariants,
two-plus bare depth-0 lines); §14.3 path-expansion conflicts; §14.4 duplicate
object keys (strict → error, non-strict → LWW).

**Examples — strict mode violations:**

**Inline array count mismatch** (declared 3, got 2):

```toon
items[3]: apple,banana
```

**Expected error:** declared 3 elements but got 2

**Tabular row width mismatch** (declared 2 fields, got 3):

```toon
users[1]{id,name}:
  1,Alice,extra
```

**Expected error:** row width mismatch (declared 2, got 3)

**Invalid escape sequence:**

```toon
msg: "bad\xescape"
```

**Expected error:** invalid escape sequence `\x`

**Why it matters — truncation detection.** These width/count checks are exactly
what make TOON *self-checking*: a truncated or injected row is a length/width
mismatch, not silently short data (§15). Error type/code/message are
implementation-defined.

**Our implementation & engineering win — structured completeness reports.** Both
runtimes and `tq` expose these mismatches as a **structured report** — `tq check`,
Rust `detect_truncation_with_options` / `detect_toonl_truncation`, JS
`detectTruncation` — with stable fields `complete`, `kind`, `line`, `declared`,
`actual`, `message`. That turns §14.1's "MUST error" into a machine-readable
diagnosis (declared-vs-actual with a line number) callers can act on. The API and
its rationale are documented in
[`toon-spec-reddb-flavored.md`](toon-spec-reddb-flavored.md#detecttruncation--structured-completeness-reports).

## §15 Security Considerations

**What it defends.** Quoting (§7.2) mitigates injection/ambiguity; strict checks
(§14) detect truncation and injected rows via length/width mismatches. Encoders
SHOULD avoid excessive memory on large inputs (stream tabular rows where
feasible). Control characters in quoted strings are preserved as data — encoders
MUST NOT strip them — and downstream consumers rendering into terminals/logs/markup
are advised to sanitize at that boundary.

**Our implementation & engineering win — the depth guard.** The official §15 does
not bound nesting depth; a pathologically deep document can exhaust a recursive
decoder's stack. Our implementations add a **depth guard** (`max_depth`, default
1000 in both `ParseOptions` and `EncodeOptions`) that rejects over-deep documents
with a structured error instead of crashing, configurable to `0` for trusted
input, with checked encode entry points for untrusted values. It changes no
decoded value — a within-limit document decodes identically. See the flavored
spec's [Depth guard](toon-spec-reddb-flavored.md#depth-guard).

## §16 Internationalization

Full Unicode in keys and values (subject to quoting/escaping); no locale-dependent
number/boolean formatting (no thousands separators). Both runtimes emit
locale-independent canonical numbers (§2) and pass the Unicode/emoji examples from
Appendix A.

## §17 IANA Considerations

Provisional media type `text/toon`, extension `.toon`, always UTF-8; no
registration requested yet. (Our streaming layer uses the parallel provisional
`application/toonl` / `.toonl`, defined in [`toonl.md`](toonl.md), not by this
spec.)

## §18 Versioning and Extensibility

**What it defends.** Backward-compatible evolutions preserve headers, quoting, and
indentation semantics; reserved structural characters (colon, brackets, braces,
hyphen) keep their meaning across versions; the path separator is fixed to `.`.

**Our implementation.** Our reddb-io extensions honor this extensibility contract —
they add no new sigil family, reuse the recursive-brace header grammar, and
fail-closed against strict v3.3 — so they are backward-compatible evolutions
rather than a fork. Details in
[`toon-spec-reddb-flavored.md`](toon-spec-reddb-flavored.md).

## §19 Intellectual Property Considerations

The official spec is MIT-licensed with no known patent disclosures, and is a
community specification (not a formal standards-track document). This companion
respects that license: it attributes the source and the exact submodule pin
(`f55b93ac489f297ff597d95e4c19ae84675eaeb7`) and derives only with attribution.
Our implementations and this repository are likewise MIT-licensed.

## Appendices A–F (Informative)

- **Appendix A — Examples.** Objects, nested objects, primitive/array-of-array/
  tabular/mixed arrays, objects-as-list-items, nested tabular inside a list item,
  delimiter variations (tab and pipe), quoted-colon disambiguation, error cases,
  edge cases (empty string, empty array, numeric-like strings, deep nesting,
  Unicode/emoji, big numbers), quoted keys with arrays, and key-folding/expansion
  round-trips. We treat these as executable expectations wherever the shared corpus
  covers them.
- **Appendix B — Parsing Helpers.** The informative decode overview, header
  parsing (B.2), `parseDelimitedValues` (B.3, quote-aware split on the active
  delimiter only), primitive token parsing (B.4, the leading-zero/`true`/`false`/
  `null` ladder), and object/list-item parsing. Our decoders follow these sketches
  in spirit; normative behavior is §§1–16.
- **Appendix C — Test Suite and Compliance.** Points at the upstream fixtures. We
  consume them live from the submodule (see below).
- **Appendix D — Changelog** and **Appendix E — Acknowledgments and License** (MIT,
  © Johann Schopplich). We reproduce attribution here per the acknowledgment
  section.
- **Appendix F — Host Type Normalization Examples.** Language-specific (Go, JS,
  Python, Rust, Java) normalization sketches informing our §3 host-type mapping for
  the Rust and JS surfaces.

## Implementation guarantees

These are the reddb-io engineering wins that back the "100% conformance" claim, by
official section:

- **Dual-runner corpora (§§1–16, Appendix C).** The conformance suite reads
  fixtures **live from the `toon-format/spec` submodule** — **389 cases (236
  decode, 153 encode) across 22 fixture files** — and the Rust crate and
  `@reddb-io/toon` run the *same* fixtures. The two implementations cannot disagree
  about the format, and the corpus tracks upstream instead of drifting from a
  vendored copy.
- **A ratchet, not a wishlist.** `tests/toon/expected-failures.txt` lists fixtures
  the crate does not yet satisfy; entries may only ever be *removed* — and it is
  currently **empty**.
- **Decode correctness, not just non-crash (§4).** A decode case passes only when
  it parses, the decoded value equals the fixture's expected JSON, **and** our own
  canonical output decodes back to that same value. "It returned `Ok`" is not a
  pass.
- **The json-limits corpus (§2, §4).** `tests/json-limits/corpus.json`
  (`json-limits-v0.1`) pins number-boundary behavior identically across the JS and
  Rust runtimes.
- **The depth guard (§15).** `max_depth` default 1000 on parse and checked encode,
  a robustness default that changes no decoded value.
- **~98% line coverage, gated at 95%.** CI fails under 95%
  (`cargo llvm-cov --workspace --fail-under-lines 95`) alongside `cargo fmt
  --check`, `cargo clippy -D warnings`, and the full suite.
- **Registry-verified releases (§17/§19 adjacent).** Every binary ships in a
  `SHA256SUMS` manifest with a GitHub-signed build-provenance attestation, and the
  crates/package publish in lockstep from each `v*.*.*` tag — so a consumer can
  verify exactly what they install.

For the reddb-io *extensions* over this baseline, see
[`toon-spec-reddb-flavored.md`](toon-spec-reddb-flavored.md); for the streaming
layer, [`toonl.md`](toonl.md).
