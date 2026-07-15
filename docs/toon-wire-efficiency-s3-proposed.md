# TOON wire-efficiency S3 array-column grammars

Status: **FROZEN** (grammar-freeze decision recorded on issue #99, 2026-07-15).
The grammar below is frozen as proposed, with the primitive-list per-cell
length caveat explicitly accepted: list cells do not declare their own item
count; the table row count and declared leaf width remain the guardrails.
Implementation slices (#100–#103) implement exactly this grammar — JS and Rust
never design independently. This document records the prototype measurements
and the frozen grammar for issue #97 / spec #93; it is not part of the
implemented dialect until those slices land.

The prototype generator lives in `scripts/wire_efficiency_s3_prototype.mjs`.
Run:

```sh
node scripts/wire_efficiency_s3_prototype.mjs --check
```

The command generates hypothetical wires from `tests/wire-efficiency/corpora.json`
without changing library encode/decode behavior.

## Goals

- Preserve the existing extension model: decoding eventually always on,
  encoding opt-in, canonical TOON v3 output unchanged by default.
- Fail closed on strict TOON v3 decoders.
- Preserve TOON's self-checking guardrail: declared lengths and declared field
  widths must make truncation, surplus rows, and width mismatch detectable.
- Keep ineligible values in ordinary TOON v3 form.

## Primitive-array columns

Recommended header syntax:

```toon
items[300|]{id,sku,tags[;],quantity}:
  item_0001|SKU-100000|hazmat;oversize|60
```

`tags[;]` declares a primitive-list cell whose in-cell sub-delimiter is `;`.
The active row delimiter remains the table delimiter declared after the row
count (`|` above).

Eligibility:

- The containing array is eligible for normal tabular encoding except for one
  or more primitive-list fields.
- Every list item is a primitive scalar: string, number, boolean, or null.
- The sub-delimiter is declared per list column and differs from the active row
  delimiter.
- Empty lists are encoded as an empty cell. Null list cells are not eligible;
  use ordinary TOON v3 fallback if the field itself can be null instead of an
  array.

Quoting and escaping:

- A list item follows ordinary scalar cell quoting first.
- A string item containing the active row delimiter, newline, leading/trailing
  whitespace, or quote syntax must be quoted by the existing TOON cell rules.
- A string item containing the list sub-delimiter is quoted; the sub-delimiter
  inside quotes is data, not a separator.
- If the existing scalar quoting rules cannot represent an item
  unambiguously, the encoder must fall back to ordinary TOON v3 for the whole
  table rather than emit a lossy cell.

Guardrail:

- The parent `[N|]` row count still checks the number of table rows.
- The `{fields}` list still checks row width.
- The list column adds an explicit type declaration and sub-delimiter, but it
  does not declare each list length. A malformed quoted subcell can be detected
  by the quote scanner; a semantically missing final list item inside a still
  well-formed cell is not independently count-checked. This is the only weaker
  guardrail relative to expanded TOON v3 arrays, and should be raised during
  grammar freeze.

## Object-array columns: child tables

Recommended header syntax:

```toon
orders[100|]{id,customer,items{sku,quantity,components{part,lot,ok}}}:
  ord_0001|cust_022|3
    SKU-0-0|7|2
      part-0|lot-2916|true
      part-1|lot-7512|true
```

`items{...}` declares a child-table column. Each parent row appends the child
row count in the position of that child column. The next indented block must
contain exactly that many child rows. A child row may itself contain a
child-table count, recursively.

Uniform-length matrices use the same shape, not a separate grammar:

```toon
matrix[150|]{values[8|]}:
  1.135|1.34|1.164|1.376|0.535|0.833|-0.242|-0.63
```

Here `values[8|]` means each matrix row has exactly eight primitive cells. The
prototype shows this shape remains worse than minified JSON on tokens, so it is
covered for grammar completeness but not recommended as a primary optimization.

Eligibility:

- Every child-table value is an array.
- Each child row is a non-empty object with the same recursive key set as the
  first child row at that level.
- Leaves are primitive scalars or recursively eligible primitive-list /
  child-table columns.
- Heterogeneous arrays, nullable child arrays, sparse rows, and mixed scalar /
  object child values fall back to ordinary TOON v3.

Guardrail:

- The parent table count checks parent rows.
- Each child-table count checks the number of child rows under that parent.
- Each child header checks child row width.
- Recursive child counts make truncation and surplus rows local parse errors,
  rather than relying on reader inference.

## Error taxonomy

Parsers should report line-numbered parse errors for:

- `E_ARRAY_COLUMN_BAD_HEADER`: malformed list or child-table declaration.
- `E_ARRAY_COLUMN_EMPTY_GROUP`: empty child field group.
- `E_ARRAY_COLUMN_DUPLICATE_PATH`: duplicate leaf path after recursive
  expansion.
- `E_ARRAY_COLUMN_BAD_SUB_DELIMITER`: missing sub-delimiter, delimiter equal
  to the active row delimiter, or unsupported delimiter token.
- `E_ARRAY_COLUMN_UNCLOSED_QUOTE`: quoted subcell never closes.
- `E_ARRAY_COLUMN_ROW_WIDTH`: row cell count differs from declared leaf width.
- `E_ARRAY_COLUMN_CHILD_COUNT`: actual child rows differ from the per-row
  child count.
- `E_ARRAY_COLUMN_INDENT`: child row indentation does not match the declared
  nesting level.

Encoders should not raise these for ordinary data in default mode. They should
fall back to standard TOON v3 when eligibility fails.

## Measurements

Local measurements use `o200k_base` through the existing optional tokenizer
cache. The rightmost column preserves the historical spec #93 token baselines;
current local tokenizer counts differ, so both are recorded.

| Scenario | Proposed wire | JSON bytes | TOON v3 bytes | Proposed bytes | Proposed bytes vs JSON | JSON tokens | TOON v3 tokens | Proposed tokens | Proposed tokens vs JSON | Spec #93 tokens |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| tagged-300 | primitive-array-column | 24,794 | 25,359 | 12,784 | -48.4% | 8,113 | 10,181 | 5,723 | -29.5% | JSON 6,506 / TOON 8,698 / hyp 4,325 |
| tree3-100 | child-table | 37,076 | 37,889 | 19,076 | -48.5% | 13,370 | 13,556 | 9,305 | -30.4% | JSON 11,953 / TOON 13,284 / hyp 7,484 |
| matrix-150x8 | matrix-as-child-table | 7,616 | 8,667 | 7,629 | 0.2% | 4,803 | 5,702 | 5,108 | 6.4% | JSON 2,406 / TOON 3,305 / hyp 2,707 |

Result: primitive-list cells and child tables remain worth pursuing. The matrix
form is expressible in the same grammar but should not be marketed as a token
win.

## LLM-readability sanity check

Executed on 2026-07-15 as a small single-pass structural retrieval check over
control, truncated, extra-row, and width-mismatch scenarios. The proposed wire
was compared with ordinary TOON v3 and minified JSON. The check asked whether
the document is structurally valid and which guardrail was violated.

| Format | Control | Truncated | Extra rows | Width mismatch |
| --- | --- | --- | --- | --- |
| Proposed | pass | pass | pass | pass |
| TOON v3 | pass | pass | pass | pass |
| Minified JSON | pass | miss | miss | miss |

Interpretation: the child-table design preserves TOON's explicit shape checks.
The primitive-list design preserves table row count and width checks but has the
known per-cell list-length caveat documented above.
