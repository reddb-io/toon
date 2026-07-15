# TOON — reddb-io extensions

This document is the normative specification of the TOON dialect implemented by
this repository. It answers three questions precisely: which official
specification we implement, what we add on top of it, and what compatibility is
guaranteed between the two.

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY, and OPTIONAL are to be interpreted as described in RFC 2119.

## Baseline: the official TOON specification

The official TOON specification is `SPEC.md` in
[toon-format/spec](https://github.com/toon-format/spec) (Working Draft v3.3),
vendored in this repository as the `vendor/toon-spec` git submodule. The
submodule pin is the exact revision our conformance suite runs against; both
implementations (Rust crate, JS package) pass **100% of the official spec
corpus** at that pin, and CI enforces it.

Unless explicitly stated in this document, TOON v3.3 governs. This document
changes no v3.3 semantics: every valid v3.3 document decodes here with
identical meaning, and the **default encoder output is canonical TOON v3.3,
byte-identical to a spec-only implementation** — the extensions below never
appear in output unless explicitly enabled.

## Extension model

Both extensions follow the same asymmetric rule:

- **Decoding is always on.** A decoder in this repository MUST accept the
  extended forms without any flag.
- **Encoding is opt-in.** An encoder MUST NOT emit an extended form unless the
  caller enabled it. With no options set, output is canonical v3.3.
- **Fail-closed on strict v3 decoders.** Each extended form is a syntax error
  for a spec-only v3.3 decoder — a document using them is rejected, never
  silently decoded into a different shape.
- **Lossless round-trip, unconditionally.** `decode(encode(x, opts)) == x` for
  every JSON value `x` and every combination of extension options. Values that
  do not fit an extension's eligibility rule fall back to standard v3.3 forms.

### Enabling emission, per surface

| Surface | Nested tabular headers | Keyed-map collapse |
| --- | --- | --- |
| JS — `serialize(value, opts)` | `{ nestedTabularHeaders: true }` | `{ keyedMapCollapse: true }` |
| Rust — `to_toon_with_options(EncodeOptions)` | `nested_tabular_headers: true` | `keyed_map_collapse: true` |
| `tq` (TOON output) | `--nested-tabular-headers` | `--keyed-map-collapse` |

## Extension 1 — Nested tabular headers

*Origin: upstream RFC [toon-format/spec#46](https://github.com/toon-format/spec/issues/46).*

v3.3's tabular form (`key[N]{fields}:`) requires every column to be a
primitive. This extension lets a column itself be a uniform nested object,
declared recursively in the header as `field{sub1,sub2}`. Rows stay flat
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

- The field list grammar becomes recursive: a field is either a key, or a key
  followed by a braced field list (`customer{name,country}`), to any depth.
- Row arity counts **leaf** columns. A nested group consumes exactly its leaf
  count of cells per row, in header order.
- Malformed nested headers (unbalanced braces, empty groups, duplicate leaf
  paths) MUST be reported as parse errors with the header's line number.
- An encoder with the option enabled emits this form only when every record in
  the array has the same shape recursively (same key sets at every level, all
  leaves primitive). Any mismatch falls back to the standard expanded list
  form — never a hard error.

## Extension 2 — Keyed-map collapse

*Origin: upstream RFC [toon-format/spec#57](https://github.com/toon-format/spec/issues/57).*

Arrays of uniform objects get table-collapse in v3.3; keyed object maps with
uniform values do not, so every field name repeats once per entry. This
extension gives uniform maps the same treatment, reusing the recursive-brace
header grammar — no new sigil family:

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
  segment**. A strict v3.3 decoder rejects it (fail-closed) instead of reading
  a different shape.
- Each row is `mapKey: cells`, one line per entry, indented one level. Map keys
  in row position follow the standard v3.3 key-quoting rules.
- Encoder eligibility is deterministic: the object has at least two entries,
  every entry value is a non-empty object, every entry has the same key set as
  the first entry, and each header leaf is primitive. Nested (recursive) leaves
  are eligible only when nested tabular headers are **also** enabled.
- Non-uniform maps stay in the ordinary v3.3 object form. Round-trip is
  lossless in every case.

## Conformance

The shared corpora under `tests/` pin both implementations to identical
behavior for the extensions (encode bytes and decode values), alongside the
official corpus for the v3.3 baseline. The `tq` golden tests cover the CLI
flags end-to-end.

## Relationship to TOONL

TOONL ([v0.1](toonl-v0.1.md), [v0.2](toonl-v0.2.md)) is an independent
line-oriented streaming extension with its own versioning; it is unaffected by
this document. The close-transform continues to target TOON v3.3 documents and
does not emit the forms defined here.
