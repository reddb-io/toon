# Proposal — Child tables + matrix (object-array columns)

**Stage:** 4 — graduated (landed via [#102](https://github.com/reddb-io/toon/pull/102) / [#103](https://github.com/reddb-io/toon/pull/103))
**Status:** graduated into `toon-reddb-spec.md` as Extension 4; decode always-on, encode opt-in, fail-closed. The matrix form is documented as **not recommended** for a token win.
**Spec section:** [Extension 4 — Object-array columns](../toon-reddb-spec.md#extension-4--object-array-columns)
**Upstream RFC:** —
**Repo issues / PRs:** [#99](https://github.com/reddb-io/toon/issues/99) (grammar freeze), [#102](https://github.com/reddb-io/toon/pull/102), [#103](https://github.com/reddb-io/toon/pull/103); spec [#93](https://github.com/reddb-io/toon/issues/93)

## Motivation

Uniform object arrays often carry a field that is itself an **array of uniform
objects** — `orders[].items[]`, and `items[].components[]` below that. TOON v3.3
must expand the parent rows because the child array is not primitive, which
destroys the parent table's amortization for deeply structured, repetitive data
(the shape where TOON should win most).

## Design / grammar

Keep the parent table and emit the child rows immediately below the parent row;
the parent cell stores the **child row count**:

```toon
orders[2|]{id|customer|items{sku|quantity|components{part|lot|ok}}}:
  ord_001|cust_a|2
    sku_1|3|2
      part_a|lot_1|true
      part_b|lot_2|false
    sku_2|1|0
  ord_002|cust_b|0
```

This decodes to the same JSON shape as the expanded v3.3 list form. The grammar
is **recursive**:

- `field{child,fields}` in a tabular header MAY denote a nested object column or
  a child-table column. The row cell disambiguates the child-table case: it is a
  non-negative decimal count, and the following rows are indented one level
  deeper. A child row may itself contain a child-table count, recursively.
- Each child table uses the same active delimiter as the containing table.
- A child row count of `0` emits no child rows.

Eligibility (deterministic): (1) the containing array is otherwise a tabular
object array; (2) each child-table field value is an array; (3) across all rows,
every element of that child array is a non-empty object with the same key set;
and (4) nested child-table fields satisfy the same rules recursively. A scalar
child value, a mixed object/scalar child array, a heterogeneous child object
shape, or a depth violation falls back losslessly to ordinary TOON v3.3.

### Matrix form — same grammar, *not recommended for a token win*

Uniform-length primitive matrices use the **same shape**, not a separate grammar
— a field value shaped as a uniform non-empty list of primitive lists:

```toon
matrix[2|]{values[3|]}:
  1|2|3
  4|5|6
```

`values[3|]` declares a fixed-width list cell: each row has exactly three
primitive cells separated by the active delimiter. The single fixed-width field
decodes back to a row array, not an object wrapper.

**This form is covered for grammar completeness but is explicitly not
recommended as a token optimization.** The prototype shows the matrix wire
remains worse than minified JSON on tokens (see measurements), so it should not
be marketed as a token win — it exists to prove the matrix shape reuses the same
opt-in extension surface and round-trips through both implementations.

### Guardrails and error taxonomy

The parent table count checks parent rows; each child-table count checks the
number of child rows under that parent; each child header checks child row width.
Recursive child counts make truncation and surplus rows **local parse errors**
rather than reader inference — a strictly stronger guardrail than the
primitive-list case. Line-numbered errors include `E_ARRAY_COLUMN_EMPTY_GROUP`,
`E_ARRAY_COLUMN_DUPLICATE_PATH`, `E_ARRAY_COLUMN_CHILD_COUNT` (actual child rows
differ from the per-row count), and `E_ARRAY_COLUMN_INDENT` (child indentation
does not match the declared nesting level).

## How to test it

- JS: `serialize(value, { objectArrayColumns: true })`
- Rust: `to_toon_with_options(EncodeOptions { object_array_columns: true, .. })`
- `tq`: `--object-array-columns`

Decoding is always-on. The `tq` golden tests cover `--object-array-columns`
end-to-end; the extension corpus covers encode/decode and fall-back. The
prototype (`scripts/wire_efficiency_s3_prototype.mjs --check`) generated the
frozen-grammar wires from `tests/wire-efficiency/corpora.json` during design.

## Measured numbers

From the frozen-grammar prototype (local `o200k_base`; historical spec
[#93](https://github.com/reddb-io/toon/issues/93) baselines preserved in the last
column):

| Scenario | Wire | JSON bytes | TOON v3 bytes | Proposed bytes | Bytes vs JSON | JSON tokens | TOON v3 tokens | Proposed tokens | Tokens vs JSON | Spec #93 tokens |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| tree3-100 | child-table | 37,076 | 37,889 | 19,076 | −48.5% | 13,370 | 13,556 | 9,305 | −30.4% | JSON 11,953 / TOON 13,284 / hyp 7,484 |
| matrix-150x8 | matrix-as-child-table | 7,616 | 8,667 | 7,629 | +0.2% | 4,803 | 5,702 | 5,108 | +6.4% | JSON 2,406 / TOON 3,305 / hyp 2,707 |

Result: **child tables are a real win** — roughly −48% bytes and −30% tokens
versus JSON on the recursive `tree3-100` corpus. The **matrix form is byte-neutral
and token-negative** against minified JSON, which is exactly why it is documented
as not recommended for a token win. The shipped-extension figures in the spec's
[Extension corpus measurements](../toon-reddb-spec.md#extension-corpus-measurements)
report the same corpora through the final implementation.

## LLM-readability sanity check

Executed 2026-07-15 as a small single-pass structural retrieval check over
control, truncated, extra-row, and width-mismatch scenarios, comparing the
proposed wire with ordinary TOON v3 and minified JSON. The check asked whether
the document is structurally valid and which guardrail was violated:

| Format | Control | Truncated | Extra rows | Width mismatch |
| --- | --- | --- | --- | --- |
| Proposed | pass | pass | pass | pass |
| TOON v3 | pass | pass | pass | pass |
| Minified JSON | pass | miss | miss | miss |

Interpretation: the child-table design preserves TOON's explicit shape checks —
recursive counts make truncation and surplus rows detectable, where minified JSON
silently misses all three failure modes. (The primitive-list design preserves row
count and width checks but carries the per-cell list-length caveat documented in
the [primitive-array columns](primitive-array-columns.md) proposal.) These are the
checks that [detectTruncation](detect-truncation.md) exposes as an API.

## Why it is a good decision

For the deeply structured, repetitive data where TOON should win most, child
tables recover the amortization that v3.3 expansion throws away, at a large
measured saving, with a **stronger** guardrail than primitive-list cells (every
child count is checked). The matrix form is included honestly: it rides the same
opt-in surface and round-trips, but it is a token *loss*, so the docs mark it
not-recommended rather than pretending otherwise. Every ineligible shape falls
back losslessly to v3.3.

## Stage transitions

- **Stage 0 — idea:** wire-efficiency program, spec [#93](https://github.com/reddb-io/toon/issues/93).
- **Stage 1 — measured proposal:** prototype in `scripts/wire_efficiency_s3_prototype.mjs`, corpus measurements, LLM sanity check.
- **Stage 2 — frozen grammar:** grammar-freeze decision on [#99](https://github.com/reddb-io/toon/issues/99), 2026-07-15; matrix shape covered for completeness only.
- **Stage 3 — implemented opt-in:** landed via [#102](https://github.com/reddb-io/toon/pull/102) / [#103](https://github.com/reddb-io/toon/pull/103); `objectArrayColumns` / `object_array_columns` / `--object-array-columns`.
- **Stage 4 — graduated:** [Extension 4](../toon-reddb-spec.md#extension-4--object-array-columns).

## Links

- Spec section: [Extension 4 — Object-array columns](../toon-reddb-spec.md#extension-4--object-array-columns)
- Repo: issue [#99](https://github.com/reddb-io/toon/issues/99), spec [#93](https://github.com/reddb-io/toon/issues/93); PRs [#102](https://github.com/reddb-io/toon/pull/102), [#103](https://github.com/reddb-io/toon/pull/103)
- Related: [Primitive-array columns](primitive-array-columns.md) (same S3 grammar freeze), [detectTruncation](detect-truncation.md) (surfaces these guardrails as an API).
