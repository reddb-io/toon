# Proposal — Nested tabular headers

**Stage:** 4 — graduated
**Status:** graduated into `toon-reddb-spec.md` as Extension 1; decode always-on, encode opt-in, fail-closed.
**Spec section:** [Extension 1 — Nested tabular headers](../toon-reddb-spec.md#extension-1--nested-tabular-headers)
**Upstream RFC:** [toon-format/spec#46](https://github.com/toon-format/spec/issues/46)
**Repo issues / PRs:** —

## Motivation

TOON v3.3's tabular form `key[N]{fields}:` is the format's biggest token win —
the field header is written once and amortized over `N` rows. But v3.3 requires
every column to be a **primitive**. The moment one column is itself a small
uniform object (`customer: {name, country}`), the whole array falls back to the
expanded list form, and the amortization is lost: every nested key repeats on
every row. Real payloads (orders with an embedded customer, events with an
embedded actor) hit this constantly.

## Design / grammar

The field-list grammar becomes **recursive**: a field is either a key, or a key
followed by a braced field list, to any depth. Rows stay flat,
delimiter-separated lines; the header alone encodes the nested shape.

```toon
orders[2]{id,customer{name,country},total}:
  1,Ada,UK,10.5
  2,Bob,US,20
```

decodes to:

```json
{"orders": [
  {"id": 1, "customer": {"name": "Ada", "country": "UK"}, "total": 10.5},
  {"id": 2, "customer": {"name": "Bob", "country": "US"}, "total": 20}
]}
```

The v3.3-equivalent expanded form (no extension) is:

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

- Row arity counts **leaf** columns. A nested group consumes exactly its leaf
  count of cells per row, in header order.
- Malformed nested headers (unbalanced braces, empty groups, duplicate leaf
  paths) are parse errors reported with the header's line number.
- **Fail-closed:** a strict v3.3 decoder rejects `field{...}` inside a tabular
  header, so it never silently reads a different shape.
- **Encode opt-in:** the form is emitted only when every record has the same
  recursive shape (same key sets at every level, all leaves primitive). Any
  mismatch falls back to the standard expanded list form — never a hard error.

## How to test it

Enable emission per surface:

- JS: `serialize(value, { nestedTabularHeaders: true })`
- Rust: `to_toon_with_options(EncodeOptions { nested_tabular_headers: true, .. })`
- `tq`: `--nested-tabular-headers`

Decoding needs no flag on any surface. The shared extension corpus under
`tests/` covers encode bytes and decode values including the eligibility and
fail-closed cases, run identically by the JS package and the Rust crate; the
`tq` golden tests cover the flag end-to-end.

## Measured numbers

The win grows with row count (the nested header is amortized once) and with the
number of nested leaves per row. On uniform nested payloads it recovers exactly
the amortization that the expanded list form throws away; on non-uniform data it
is a no-op because the encoder falls back. Measure against your own corpus with
`scripts/research_token_benchmark.py` (tokenized with `o200k_base`), as the spec
notes for all extension figures.

## Why it is a good decision

It reuses the existing recursive-brace header grammar rather than inventing a
new sigil, so the mental model is "the header can nest, the rows stay flat." The
cost is a slightly denser header to parse; the buy is keeping the array tabular
whenever a column happens to be a uniform object. Because it fails closed and
falls back losslessly, enabling it can never corrupt a strict-decoder pipeline.

## Stage transitions

- **Stage 0–1 — idea / measured proposal:** upstream RFC [toon-format/spec#46](https://github.com/toon-format/spec/issues/46).
- **Stage 2 — frozen grammar:** recursive braced field list, leaf-arity rule.
- **Stage 3 — implemented opt-in:** `nestedTabularHeaders` / `nested_tabular_headers` / `--nested-tabular-headers`.
- **Stage 4 — graduated:** [Extension 1](../toon-reddb-spec.md#extension-1--nested-tabular-headers).

## Links

- Spec section: [Extension 1 — Nested tabular headers](../toon-reddb-spec.md#extension-1--nested-tabular-headers)
- Upstream RFC: https://github.com/toon-format/spec/issues/46
- Related: [Keyed-map collapse](keyed-map-collapse.md) reuses the same recursive-brace header grammar.
