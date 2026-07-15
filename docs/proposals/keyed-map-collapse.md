# Proposal — Keyed-map collapse

**Stage:** 4 — graduated
**Status:** graduated into `toon-reddb-spec.md` as Extension 2; decode always-on, encode opt-in, fail-closed.
**Spec section:** [Extension 2 — Keyed-map collapse](../toon-reddb-spec.md#extension-2--keyed-map-collapse)
**Upstream RFC:** [toon-format/spec#57](https://github.com/toon-format/spec/issues/57)
**Repo issues / PRs:** —

## Motivation

v3.3 gives **arrays** of uniform objects the table-collapse win, but keyed
object **maps** with uniform values get nothing — every field name repeats once
per entry. A map of `id -> {first, last}` with 50 entries writes `first` and
`last` 50 times each. Maps keyed by an id are extremely common (lookup tables,
per-user records), so this is a recurring, avoidable cost.

## Design / grammar

Give uniform maps the same collapse, reusing the recursive-brace header grammar
— **no new sigil family**:

```toon
people{first,last}:
  joe: Joe,Schmoe
  mary: Mary,Jane
```

decodes to an **object map** (not an array):

```json
{"people": {
  "joe":  {"first": "Joe",  "last": "Schmoe"},
  "mary": {"first": "Mary", "last": "Jane"}
}}
```

The v3.3-equivalent expanded form (no extension):

```toon
people:
  joe:
    first: Joe
    last: Schmoe
  mary:
    first: Mary
    last: Jane
```

Rules:

- The header is `key{fields}:` — object-typed because there is **no `[N]`
  segment**. A strict v3.3 decoder rejects it (fail-closed) instead of reading a
  different shape.
- Each row is `mapKey: cells`, one line per entry, indented one level. Map keys
  in row position follow the standard v3.3 key-quoting rules.
- Non-uniform maps stay in ordinary v3.3 object form; round-trip is lossless in
  every case.
- Nested (recursive) leaves are eligible only when
  [nested tabular headers](nested-tabular-headers.md) is **also** enabled.

## How to test it

- JS: `serialize(value, { keyedMapCollapse: true })`
- Rust: `to_toon_with_options(EncodeOptions { keyed_map_collapse: true, .. })`
- `tq`: `--keyed-map-collapse`

Decoding is always-on. The extension corpus covers encode/decode plus the
single-entry (not collapsed) and non-uniform (fall-back) cases across both
implementations.

## Measured numbers

The saving is the same amortization as tabular arrays — each header field is
written once instead of once per entry — and grows with entry count. It is a
no-op below the two-entry guardrail (see below) and for non-uniform maps.
Reproduce with `scripts/research_token_benchmark.py` (`o200k_base`).

## Why it is a good decision

### The deliberate absence of an `[N]` entry count — and its trade-off

This is the notable trade-off and the reason this form is **not** just "arrays,
but for maps." A tabular array header carries `[N]`, so a truncated array is a
structural mismatch the decoder catches. A collapsed map header is
`people{first,last}:` with **no `[N]` entry count**: the number of entries is
whatever number of `mapKey: cells` rows follow. That is intentional — map order
and entry count are not part of a JSON object's identity the way array length
is, and inventing an entry count would force the encoder to commit to a count
that the data model does not consider meaningful. The accepted cost is that a
collapsed map does **not** get the "declared-vs-actual entry count" guardrail
that a tabular array gets; a truncated map is detected only if a partial final
row is itself malformed. Per-row width is still checked by `{fields}`.

### The entry-count guardrail (≥2 entries)

An encoder emits the collapsed form only when **all** hold: (1) the object has
**at least two entries**; (2) every entry value is a non-empty object; (3) every
entry has the same key set as the first; and (4) each header leaf is primitive
(or eligible per the nested-headers rule). Rule 1 is a token/clarity balance: for
one entry the collapsed header plus one row does not beat the ordinary object
form on tokens, and it costs the reader a header to parse for a single record.
So a size-one uniform map stays in plain v3.3 form even with the option on. This
keeps encoder output stable rather than flipping shape at the size-one boundary;
round-trip is lossless either way because a non-collapsed map is just v3.3.

## Stage transitions

- **Stage 0–1 — idea / measured proposal:** upstream RFC [toon-format/spec#57](https://github.com/toon-format/spec/issues/57).
- **Stage 2 — frozen grammar:** `key{fields}:` object-typed header, per-entry row.
- **Stage 3 — implemented opt-in:** `keyedMapCollapse` / `keyed_map_collapse` / `--keyed-map-collapse`.
- **Stage 4 — graduated:** [Extension 2](../toon-reddb-spec.md#extension-2--keyed-map-collapse).

## Links

- Spec section: [Extension 2 — Keyed-map collapse](../toon-reddb-spec.md#extension-2--keyed-map-collapse), including [the entry-count guardrail and its trade-off](../toon-reddb-spec.md#the-entry-count-guardrail-and-its-trade-off).
- Upstream RFC: https://github.com/toon-format/spec/issues/57
- Related: [Nested tabular headers](nested-tabular-headers.md) (required for nested leaves).
