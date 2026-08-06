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

## Breaking changes

### 1. Path expansion removed from the spec

**What changed.** In v3.3 the decoder exposed a spec option
`expandPaths: "safe"` (§13.4) that split dotted keys back into nested objects
*after* base parsing. v4.1 removes path expansion from the specification
entirely. Canonical decode treats a dotted key as a single literal key. Our
decoders retain `expandPaths` only as a **non-normative legacy shim** (still
defaulting off); output it produces is not v4.1-conformant, and it should not
be relied on for new work.

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
have accepted now raise a positioned `ToonError` / `ToonError` under the
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
keep their contract — decode always-on, encode opt-in, fail-closed, lossless
round-trip. No migration is required for these; only the baseline they layer
over moved from v3.3 to v4.1:

- Primitive-array columns (`primitiveArrayColumns`) — upstream RFC
  [spec#49](https://github.com/toon-format/spec/issues/49) is still open.
- Delimiter choice (`delimiter`) — upstream RFC
  [spec#48](https://github.com/toon-format/spec/issues/48) is still open.
- Object-array columns / child tables (`objectArrayColumns`).
- Cyclic discriminated arrays (`cyclicDiscriminatedArrays`).
- Depth guard (`maxDepth`) and the `detectTruncation` completeness report.
- TOONL streaming (`docs/toonl-reddb-spec.md`), whose close-transform now
  targets canonical TOON v4.1 documents.

See [`docs/toon-reddb-spec.md`](toon-reddb-spec.md) for the normative
definition of the surviving extensions.
