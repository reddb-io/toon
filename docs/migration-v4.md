# Migrating to the TOON v4.1 baseline

**tl;dr.** This repository rebased its baseline from the official TOON working
draft **v3.3** to the official **v4.1** specification (see
[ADR 0005](../.red/adr/0005-rebase-on-spec-v4-1-with-event-based-decoders.md)).
Two mechanisms the reddb-io flavor used to define — nested tabular headers and
keyed-map collapse — were absorbed into the official spec, so the official
syntax now governs them. Two spec features were **removed**: encoder *key
folding* and decoder *path expansion*. Strict mode was **hardened**. This
document enumerates every breaking change for downstream consumers, with the
concrete before/after decode (or encode) behavior for each.

The pinned checkpoint moved with the rebase: the `vendor/toon` and
`vendor/toon-spec` submodules are bumped to the **v4.1.1** commits
(`a9e6d97` / `62f16b3`). Conformance is proven against the renormalized v4
fixture corpus.

## Who this is for

- **Library consumers** (`@reddb-io/toon`, `reddb_io_toon` crate) that decode
  or encode TOON in application code.
- **`tq` users** with scripts that depend on a particular decode result.
- **Anyone storing TOON documents** produced by a v3.3-era encoder that must
  now be read by a v4.1 decoder.

If you only ever decode canonical, spec-conformant TOON that never used dotted
keys and never relied on lenient numeric parsing, no change is required: valid
v4.1 documents are a superset-compatible evolution and the default encoder
output is now canonical **v4.1**.

## TypeScript cutover

The package root exposes the canonical `decode` and `encode` names; the
deprecated root `parse` and `serialize` aliases have been removed. Legacy
parsing remains available from an explicit subpath.

Before the cutover, a root import plus path expansion could produce a nested
object:

```js
import { parse } from '@reddb-io/toon'

parse('a.b: 1', { expandPaths: 'safe' })
// { a: { b: 1 } }
```

After the cutover, canonical decode preserves the dotted key literally:

```js
import { decode } from '@reddb-io/toon'

decode('a.b: 1')
// { 'a.b': 1 }
```

If a stored document temporarily requires the old observable behavior, make
that dependency visible at the import boundary:

```js
import { parse } from '@reddb-io/toon/legacy'

parse('a.b: 1', { expandPaths: 'safe' })
// { a: { b: 1 } }
```

New code should use `decode`, `decodeFromLines`, `decodeStream`, `encode`, or
`encodeLines`. The `reviver` option is an experimental TypeScript-only frontier
pinned to upstream PR #294; it is not part of the v4.1 migration contract.

## Rust cutover

Suffix-free functions and common model methods are canonical v4.1. Use
`decode`, `decode_with_options`, `decode_reader`, `decode_iter`, `encode`, and
`encode_with_options`; `Value::parse_toon`, `Document::parse`, and canonical
output methods delegate to the same codec.

Before the cutover, path expansion was part of the ordinary parser options.
After the cutover, the same input has a literal key:

```rust
use reddb_io_toon::decode;

let value = decode("a.b: 1\n")?;
assert_eq!(value.to_json_value(), serde_json::json!({"a.b": 1}));
# Ok::<(), reddb_io_toon::DecodeError>(())
```

An explicit compatibility read uses a method containing `legacy`:

```rust
use reddb_io_toon::{LegacyParseOptions, Value};

let value = Value::parse_legacy_with_options(
    "a.b: 1\n",
    LegacyParseOptions { expand_paths: true, ..LegacyParseOptions::default() },
)?;
assert_eq!(value.to_json_value(), serde_json::json!({"a": {"b": 1}}));
# Ok::<(), reddb_io_toon::DecodeError>(())
```

Methods named `to_legacy_toon*` are the corresponding old-output boundary.
Compatibility-shaped `EncodeOptions` passed to common model methods still
route through the v4.1 encoder; only explicitly named legacy methods select the
former encoder.

## Breaking changes

### 1. Path expansion removed from the spec

**What changed.** In v3.3 the decoder exposed a spec option
`expandPaths: "safe"` (§13.4) that split dotted keys back into nested objects
*after* base parsing. v4.1 removes path expansion from the specification
entirely. Canonical decode treats a dotted key as a single literal key. The
behavior survives only on the explicit TypeScript legacy subpath and Rust
methods whose names contain `legacy`; output it produces is not canonical
v4.1 and should not be relied on for new work.

**Before/after decode** of `a.b.c: 1`:

```text
# v3.3, decoder configured with expandPaths: "safe"
a.b.c: 1   ->   { "a": { "b": { "c": 1 } } }

# v4.1 default decode (strict: true, no expansion)
a.b.c: 1   ->   { "a.b.c": 1 }
```

The default decode result was already literal in v3.3 (path expansion defaulted
off there too), so nothing changes for callers who never set the option. The
break is for callers who **opted into** expansion, and for anyone decoding
documents that a v3.3 encoder produced with *key folding* on (see below): those
dotted keys are now read literally.

**Migration.** Stop passing `expandPaths`. If you need nesting from dotted
keys, expand them in your own application code after decoding.

### 2. Key folding removed from the spec

**What changed.** In v3.3 the encoder exposed `keyFolding: "safe"` (with a
`flattenDepth` bound) that collapsed chains of single-key objects into dotted
paths. v4.1 removes key folding. The v4.1 encoder never folds — the option is
ignored on the canonical encode path — so a chain of single-key objects always
serializes as an indented block.

**Before/after encode** of `{ "a": { "b": { "c": 1 } } }`:

```text
# v3.3 with keyFolding: "safe"
a.b.c: 1

# v4.1 (folding removed; the option no longer changes output)
a:
  b:
    c: 1
```

Default encoder output never folded, so callers who never set `keyFolding` see
no change. The break is for callers who **opted into** folded output: their
documents are now larger (indented blocks) but semantically identical, and they
round-trip losslessly against a v4.1 decoder without any expansion step.

### 3. Strict mode hardened

Strict mode (`strict: true`, the default on both surfaces) enforces the v4.1
authoritative error checklist fail-fast. Documents a lenient v3.3 reader may
have accepted now raise a positioned `ToonDecodeError` / `DecodeError` under the
default, and the numeric grammar is tightened so several token shapes that a
looser reader treated as numbers now decode as **strings**.

**3a. Numeric grammar.** Only JSON-style numbers are recognized as numbers.
Leading-zero integers, a leading `+`, bare fractional forms, and the
non-finite words fall outside the grammar and decode as strings:

```text
# v4.1 decode (verified)
n: 007        ->   { "n": "007" }     # not the number 7
n: +5         ->   { "n": "+5" }      # not the number 5
n: .5         ->   { "n": ".5" }      # not the number 0.5
n: Infinity   ->   { "n": "Infinity" }
n: NaN        ->   { "n": "NaN" }
```

A v3.3 reader with a looser numeric grammar may have decoded some of these as
numbers. **Migration:** if you relied on any of these token shapes being
numbers, normalize them at the producer, or coerce after decoding.

**3b. Structural checks are hard errors.** Array count/width mismatches,
duplicate sibling keys, tab-as-indentation, and the indentation-multiple
invariant are strict errors rather than best-effort recoveries:

```text
# declared 3 elements, supplied 2
items[3]: a,b            ->   ToonError "array length mismatch"

# duplicate sibling key
a: 1
a: 2                     ->   ToonError "duplicate key"   (strict)
                         ->   { "a": 2 }  (last-write-wins, strict: false)

# tab used for indentation
a:
<TAB>b: 1                ->   ToonError "invalid indentation"
```

**Migration.** Fix the source documents, or pass `{ strict: false }` (JS) /
`strict: false` (Rust) to opt back into the lenient recovery behavior
(last-write-wins on duplicates, tolerant indentation). Non-strict decoding is
explicitly a legacy-compatibility mode, not a supported long-term target.

## Absorbed mechanisms (design history, not a break)

Two mechanisms the reddb flavor previously defined were adopted by the official
spec at v4.1; the official syntax and semantics now govern them, and the
former reddb proposal documents are retained only as design history:

| Mechanism | Upstream RFC | Was | Now |
| --- | --- | --- | --- |
| Nested tabular headers | [spec#46](https://github.com/toon-format/spec/issues/46) | reddb Extension 1 | Official v4.1 (nested field groups) |
| Keyed-map collapse | [spec#57](https://github.com/toon-format/spec/issues/57) | reddb Extension 2 | Official v4.1 (keyed tabular form) |

The wire output is unchanged where it overlaps; these move from
"reddb extension" to "official spec feature." See
[`docs/proposals/nested-tabular-headers.md`](proposals/nested-tabular-headers.md)
and [`docs/proposals/keyed-map-collapse.md`](proposals/keyed-map-collapse.md).

## Surviving opt-in extensions (unchanged behavior)

The remaining reddb-io extensions were **re-expressed on the v4.1 base** and
remain encode opt-ins with lossless fallback. Primitive-array and object-array
wires decode by default and fail closed when extension handling is disabled;
cyclic reconstruction is opt-in and otherwise has a literal v4.1 reading. No
migration is required for these; only their baseline moved from v3.3 to v4.1:

- Primitive-array columns (`primitiveArrayColumns`) — local opt-in extension;
  there is no dedicated upstream RFC.
- Delimiter choice (`delimiter`) — official v4.1 configuration with local
  defaults and guidance, not userland grammar; there is no dedicated upstream
  RFC for the guidance.
- Object-array columns / child tables (`objectArrayColumns`).
- Cyclic discriminated arrays (`cyclicDiscriminatedArrays`).
- Depth guard (`maxDepth`) and the `detectTruncation` completeness report.
- TOONL streaming (`docs/toonl-reddb-spec.md`), whose close-transform now
  targets canonical TOON v4.1 documents.

See [`docs/toon-reddb-spec.md`](toon-reddb-spec.md) for the normative
definition of the surviving extensions.
