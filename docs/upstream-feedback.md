# Upstream feedback — mixed columnar arrays

Approval status: **NOT APPROVED — NOT POSTED**. The exact comment below is a
local draft for maintainer review. Posting it to `toon-format/spec` requires a
fresh human approval; after posting, record the comment URL and response in the
final section.

## Source check — 2026-08-07

| Source | Freshly checked state | Relevance |
| --- | --- | --- |
| [toon-format/spec#48](https://github.com/toon-format/spec/issues/48) | **OPEN**, last updated 2026-07-15 | The actual RFC is “v4.0: Mixed columnar arrays, objectArrayLayout, ignoreNullOrEmpty, excludeEmptyArrays.” It asks that the lossy options be unbundled and that accuracy be benchmarked alongside tokens. |
| [toon-format/spec draft PR #47](https://github.com/toon-format/spec/pull/47) | **OPEN DRAFT**, unmerged, last updated 2026-07-15; head revision [`b5ce4c6`](https://github.com/toon-format/spec/commit/b5ce4c6619665fb23e38f5ae123bb34277d78172) | Proposes primitive header cells followed by indented complex-field spill lines. The draft still bundles the two lossy options and binary guidance. |
| [toon-format/spec#49](https://github.com/toon-format/spec/issues/49) | **CLOSED / COMPLETED** on 2026-07-22 | This was the v4 tabular-generalization roadmap. Its closing comment says #48 remains open for reassessment; it is not a primitive-array-column RFC. |
| [TOON v4.1.1](https://github.com/toon-format/spec/releases/tag/v4.1.1) | **RELEASED** 2026-08-05 at [`62f16b3`](https://github.com/toon-format/spec/commit/62f16b369408180f1faf1cba7da1b46d1f336f12) | Latest released v4.1 patch and the spec revision pinned by this repository. |
| [v4.1.1 §9.3](https://github.com/toon-format/spec/blob/62f16b369408180f1faf1cba7da1b46d1f336f12/SPEC.md#L489-L530) / [§9.4](https://github.com/toon-format/spec/blob/62f16b369408180f1faf1cba7da1b46d1f336f12/SPEC.md#L531-L549) | **RELEASED syntax** | Nested field groups flatten uniform non-empty object columns into primitive cells. An array-valued or otherwise ineligible column makes the whole array use list form. |
| [v4.1.1 §9.5](https://github.com/toon-format/spec/blob/62f16b369408180f1faf1cba7da1b46d1f336f12/SPEC.md#L551-L575) | **RELEASED syntax** | Keyed tabular form handles objects whose values share a uniform shape; it does not settle mixed object arrays. |

The old local mapping of #48 to delimiter choice and #49 to primitive-array
columns was incorrect and has been removed. Delimiter selection is already an
official v4.1 mechanism; this repository only adds local defaults and guidance.

## Syntax comparison

The three layers are deliberately separate:

1. **Released v4.1.1:** `customer{name,country}` means a uniform nested
   **object** whose primitive leaves are flattened into the same row. Arrays in
   a column remain ineligible, so the encoder uses §9.4 list form.
2. **Draft #47:** a header lists only primitive fields, while complex fields are
   emitted as named spill lines below each row. It is proposed upstream syntax,
   not released syntax.
3. **reddb-io opt-ins:** `tags[;]` stores a primitive array inside one cell;
   `items{sku,quantity}` plus a numeric parent cell stores a child-row count and
   indents those child rows below it. These are implemented local extensions,
   not official v4.1 and not implementations of draft #47.

The local child-table form intentionally fails closed for a v4.1 decoder with
extension handling disabled: v4.1 interprets `items{sku,quantity}` as nested
object leaves, so the count cell/indented child rows cannot be silently decoded
as a different JSON value. The exact opt-in, round-trip, disabled-decoder, and
fallback cases are executable in
[`packages/toon/test/extensions-v4.test.mjs`](../packages/toon/test/extensions-v4.test.mjs)
and [`tests/runners/rust/toon/wire_efficiency.rs`](../tests/runners/rust/toon/wire_efficiency.rs)
with `node --test packages/toon/test/extensions-v4.test.mjs` and
`cargo test --workspace`.

## Reproducible evidence

Run `pnpm benchmark:tokens` to regenerate
[`benchmarks/results/2026-08-06-token-efficiency.md`](../benchmarks/results/2026-08-06-token-efficiency.md).
The report uses `o200k_base` through `gpt-tokenizer` and labels the synthetic
wire corpora separately from representative datasets.

| Evidence | Minified JSON | Released v4.1 | Local opt-in | Scope |
| --- | ---: | ---: | ---: | --- |
| `wire-tagged-300` | 8,113 tokens | 10,181 | primitive-array columns: 5,723 (**−29.5% vs JSON**) | Synthetic eligibility showcase generated from [`tests/corpus/wire-efficiency/corpora.json`](../tests/corpus/wire-efficiency/corpora.json). |
| `nested-uniform/openapi-petstore-paths-large` (96 records) | 8,345 tokens | 9,174 | child tables: 5,202 (**−37.7% vs JSON**) | Representative offline dataset; the local wire is 20,434 bytes vs 38,013 JSON and 36,541 released v4.1. |
| `tagged-records/activity-events-large` (120 records) | 6,386 tokens | 7,632 | primitive-array option: 7,632 (**+19.5% vs JSON**) | Representative dataset does not meet this extension's eligibility; it demonstrates canonical fallback, not a win. |

Fixture-level compatibility is covered by
[`primitive-array-columns.json`](../tests/corpus/wire-efficiency/primitive-array-columns.json)
and [`object-array-columns.json`](../tests/corpus/wire-efficiency/object-array-columns.json):
they pin exact bytes, decoded JSON, malformed-header/width/count errors, empty
children, and ineligible-shape fallback in both shipped implementations.

The available local model observation in
[`2026-07-15-retrieval-accuracy.md`](../benchmarks/results/2026-07-15-retrieval-accuracy.md)
is a small 6/8 run and does not isolate mixed-columnar syntax. It is therefore
not evidence for an accuracy claim about #47. A decision on the draft should
first compare released v4.1 list form, draft #47 spill lines, primitive-list
cells, and counted child tables on the same payloads and questions.

## Exact comment proposed for toon-format/spec#48

> We re-checked this RFC against released TOON v4.1.1 and implemented two
> related experiments in TypeScript and Rust. To keep the terms straight: these
> are evidence for the mixed-columnar discussion, not implementations of this
> draft and not released TOON syntax.
>
> Released v4.1.1 already handles uniform nested **object** columns through
> nested field groups (§9.3), but any array-valued column still disqualifies the
> table and falls back to §9.4 list form. Draft PR #47 is different: it keeps the
> primitive fields in the row and writes complex values as named spill lines.
> Our local opt-ins explored two other points in the design space:
>
> - `tags[;]` keeps an array of primitive scalars inside one tabular cell.
> - `items{sku,quantity}` plus a numeric cell stores a per-parent child-row
>   count, followed by indented child rows; this recurses for deeper child
>   arrays.
>
> Both local encoders default to canonical v4.1 and require an explicit opt-in.
> Eligible wires round-trip in both implementations, ineligible shapes fall
> back to canonical v4.1, and extension wires fail closed rather than silently
> changing meaning when child-table handling is disabled. Reproducible fixtures
> and tests:
> https://github.com/reddb-io/toon/blob/main/tests/corpus/wire-efficiency/primitive-array-columns.json
> https://github.com/reddb-io/toon/blob/main/tests/corpus/wire-efficiency/object-array-columns.json
> (`node --test packages/toon/test/extensions-v4.test.mjs` and
> `cargo test --workspace`).
>
> The current deterministic token report is generated with
> `pnpm benchmark:tokens`:
> https://github.com/reddb-io/toon/blob/main/benchmarks/results/2026-08-06-token-efficiency.md
>
> - On the synthetic `wire-tagged-300` eligibility fixture, primitive-list
>   cells use 5,723 `o200k_base` tokens versus 8,113 for minified JSON and
>   10,181 for canonical v4.1 (−29.5% vs JSON).
> - On the representative 96-record OpenAPI Petstore paths fixture, counted
>   child tables use 5,202 tokens versus 8,345 for minified JSON and 9,174 for
>   canonical v4.1 (−37.7% vs JSON); bytes are 20,434 vs 38,013 and 36,541.
> - The representative 120-record tagged-events fixture does not meet the
>   primitive-list eligibility rule, so the option falls back to canonical
>   v4.1 and remains a 19.5% token loss versus JSON. We do not want to generalize
>   from the synthetic win.
>
> The main design difference we found is the guardrail. Primitive-list cells do
> not declare an item count. Counted child tables do: truncation or surplus child
> rows is a local parse error. Draft #47's named spill lines are more general
> than either local extension, but the draft should specify equally explicit
> row-boundary and completeness checks after the lossy options are unbundled.
>
> We also do not yet have defensible mixed-columnar model-accuracy evidence. Our
> existing local observation is a small 6/8 sanity run and does not isolate this
> syntax. Before recommending one form, we would compare v4.1 list form, #47
> spill lines, primitive-list cells, and counted child tables on the same
> payloads and questions. We would be happy to contribute the fixtures and a
> head-to-head harness if that would help the reassessment.

## Upstream posting record

- Approval: pending
- Posted comment URL: not posted
- Upstream response/status: pending
