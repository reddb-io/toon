# Proposal — Primitive-array columns

**Stage:** 4 — graduated (landed via [#100](https://github.com/reddb-io/toon/pull/100) / [#101](https://github.com/reddb-io/toon/pull/101))
**Status:** graduated into `toon-reddb-spec.md` as Extension 3; decode always-on, encode opt-in, fail-closed.
**Spec section:** [Extension 3 — Primitive-array columns](../toon-reddb-spec.md#extension-3--primitive-array-columns)
**Upstream RFC:** [toon-format/spec#49](https://github.com/toon-format/spec/issues/49)
**Repo issues / PRs:** [#97](https://github.com/reddb-io/toon/issues/97), [#99](https://github.com/reddb-io/toon/issues/99) (grammar freeze), [#100](https://github.com/reddb-io/toon/pull/100), [#101](https://github.com/reddb-io/toon/pull/101); spec [#93](https://github.com/reddb-io/toon/issues/93)

## Motivation

Uniform object arrays often carry a field whose value is an array of **primitive
scalars** — `tags: ["hazmat", "oversize"]`. TOON v3.3 cannot keep the containing
array tabular, because the array-valued field is not itself primitive, so the
whole table falls back to the expanded list form and loses its amortization.
Tagged records (a row plus a small list of labels) are a very common shape.

## Design / grammar

An otherwise-tabular object array may declare such a field as a **primitive-list
cell** whose in-cell sub-delimiter is declared in brackets:

```toon
items[2]{id,tags[;],quantity}:
  item_0001,hazmat;oversize,60
  item_0002,oversize,11
```

decodes to:

```json
{"items":[{"id":"item_0001","tags":["hazmat","oversize"],"quantity":60},{"id":"item_0002","tags":["oversize"],"quantity":11}]}
```

Frozen grammar (recorded at grammar freeze on [#99](https://github.com/reddb-io/toon/issues/99), 2026-07-15):

- In an array field header, `field[;]` declares `field` as a primitive-list cell.
  The bracket content is the in-cell sub-delimiter; the encoder emits `;`, valid
  with every active row delimiter (comma, tab, pipe).
- Row cells still use the array header's **active row delimiter**. The list
  sub-delimiter splits only inside that one field's cell.
- Empty arrays encode as an empty cell. Null list cells are **not** eligible; use
  ordinary TOON v3.3 fallback if the field itself can be null.

Eligibility (deterministic): (1) the containing array is eligible for normal
tabular encoding except for one or more primitive-list fields; (2) every
primitive-list field value is an array; (3) every item is a primitive scalar
(string, number, boolean, null); and (4) the list sub-delimiter differs from the
active row delimiter. Null list fields, mixed scalar/object items, sparse rows,
and heterogeneous shapes fall back to ordinary TOON v3.3. The encoder MUST NOT
raise an error for ordinary ineligible data.

Quoting follows the scalar cell rules — an item is quoted when it would need
quoting as an ordinary row cell, or when it contains the list sub-delimiter:

```toon
items[1]{id,tags[;]}:
  1,"semi;quoted";plain
```

If existing scalar quoting cannot represent an item unambiguously, the encoder
falls back to ordinary TOON v3.3 for the whole table rather than emit a lossy
cell.

### Error taxonomy

Parsers report line-numbered parse errors for the array-column grammar,
including: `E_ARRAY_COLUMN_BAD_HEADER`, `E_ARRAY_COLUMN_BAD_SUB_DELIMITER`
(missing sub-delimiter, equal to the active row delimiter, or unsupported token),
`E_ARRAY_COLUMN_UNCLOSED_QUOTE` (quoted subcell never closes), and
`E_ARRAY_COLUMN_ROW_WIDTH` (row cell count differs from declared leaf width).
Encoders do not raise these for ordinary data in default mode; they fall back to
standard TOON v3.3 when eligibility fails.

## How to test it

- JS: `serialize(value, { primitiveArrayColumns: true })`
- Rust: `to_toon_with_options(EncodeOptions { primitive_array_columns: true, .. })`
- `tq`: `--primitive-array-columns`

Decoding is always-on. The extension corpus covers encode bytes, decode values,
quoting, and the fail-closed / fall-back cases across both implementations. The
prototype generator (`scripts/wire_efficiency_s3_prototype.mjs --check`) generated
the frozen-grammar wires from `tests/wire-efficiency/corpora.json` during design.

## Measured numbers

From the frozen-grammar prototype (local `o200k_base` via the optional tokenizer
cache); the historical spec [#93](https://github.com/reddb-io/toon/issues/93)
baselines are preserved in the last column because current local tokenizer counts
differ:

| Scenario | Wire | JSON bytes | TOON v3 bytes | Proposed bytes | Bytes vs JSON | JSON tokens | TOON v3 tokens | Proposed tokens | Tokens vs JSON | Spec #93 tokens |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| tagged-300 | primitive-array-column | 24,794 | 25,359 | 12,784 | −48.4% | 8,113 | 10,181 | 5,723 | −29.5% | JSON 6,506 / TOON 8,698 / hyp 4,325 |

Result: primitive-list cells are a real, measured win — roughly **−48% bytes**
and **−30% tokens** versus JSON on the tagged corpus — and remain worth pursuing.

## Why it is a good decision

It recovers table amortization for the extremely common "row + small list of
labels" shape, at a large measured saving, while keeping every ineligible case in
ordinary v3.3. The **one accepted weakness**, raised and accepted at grammar
freeze, is a guardrail gap: the parent `[N]` row count still checks row count and
`{fields}` still checks row width, but a primitive-list cell does **not** declare
each list's item count. A malformed quoted subcell is caught by the quote
scanner, but a semantically missing final list item inside a still-well-formed
cell is not independently count-checked. This is the only guardrail weaker than
expanded v3.3 arrays, and it was deliberately accepted because the token win is
large and the failure mode is narrow.

## Stage transitions

- **Stage 0 — idea:** wire-efficiency program, spec [#93](https://github.com/reddb-io/toon/issues/93).
- **Stage 1 — measured proposal:** prototype in `scripts/wire_efficiency_s3_prototype.mjs`, corpus measurements, issue [#97](https://github.com/reddb-io/toon/issues/97).
- **Stage 2 — frozen grammar:** grammar-freeze decision on [#99](https://github.com/reddb-io/toon/issues/99), 2026-07-15, per-cell length caveat explicitly accepted.
- **Stage 3 — implemented opt-in:** landed via [#100](https://github.com/reddb-io/toon/pull/100) / [#101](https://github.com/reddb-io/toon/pull/101); `primitiveArrayColumns` / `primitive_array_columns` / `--primitive-array-columns`.
- **Stage 4 — graduated:** [Extension 3](../toon-reddb-spec.md#extension-3--primitive-array-columns).

## Links

- Spec section: [Extension 3 — Primitive-array columns](../toon-reddb-spec.md#extension-3--primitive-array-columns)
- Upstream RFC: https://github.com/toon-format/spec/issues/49
- Repo: issues [#97](https://github.com/reddb-io/toon/issues/97), [#99](https://github.com/reddb-io/toon/issues/99), spec [#93](https://github.com/reddb-io/toon/issues/93); PRs [#100](https://github.com/reddb-io/toon/pull/100), [#101](https://github.com/reddb-io/toon/pull/101)
- Related: [Child tables + matrix](child-tables-and-matrix.md) froze in the same S3 grammar and shares the error taxonomy and LLM-readability sanity check.
