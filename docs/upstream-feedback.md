# Upstream feedback — toon-format/spec#48 and spec#49

**tl;dr.** We implemented the mechanisms discussed in the official RFCs
[toon-format/spec#48](https://github.com/toon-format/spec/issues/48) (delimiter
choice) and [toon-format/spec#49](https://github.com/toon-format/spec/issues/49)
(array-valued fields in tabular rows) end-to-end in our v3.3-compatible dialect
(JS + Rust + `tq`, 100% official conformance corpus on both implementations),
measured them with a real tokenizer (`o200k_base`), and are reporting the
results back upstream. Headline numbers: primitive-array columns
**−48.4% bytes / −29.5% tokens** vs minified JSON on a tagged-record corpus;
recursive child tables **−48.5% bytes / −30.4% tokens** on a 3-level tree
corpus — with TOON's self-checking guardrails preserved. This document is the
in-repo record of that feedback package: the measured results and the exact
comment texts approved for posting.

## Table of contents

- [Context](#context)
- [Results summary](#results-summary)
- [Trade-offs we found](#trade-offs-we-found)
- [Draft comment for spec#48 (delimiter choice)](#draft-comment-for-spec48-delimiter-choice)
- [Draft comment for spec#49 (array-valued fields)](#draft-comment-for-spec49-array-valued-fields)
- [Links](#links)

## Context

The reddb-io dialect ([`toon-reddb-spec.md`](toon-reddb-spec.md)) tracks the
official TOON v3.3 spec ([companion doc](toon-official-spec.md)) and adds
opt-in encoder extensions that are always-on and fail-closed on decode. Two of
those extensions implement, ahead of the official spec, mechanisms that are
open RFCs upstream:

- **Delimiter choice** (spec#48) — defaults and guidance over the existing
  three-delimiter mechanism; full design history in the
  [delimiter-choice proposal](proposals/delimiter-choice.md).
- **Primitive-array columns** (spec#49) — `field[;]` declares a primitive-list
  cell with an in-cell sub-delimiter; full design history in the
  [primitive-array-columns proposal](proposals/primitive-array-columns.md).
  We additionally shipped recursive **child tables** for object-array fields
  ([child-tables-and-matrix proposal](proposals/child-tables-and-matrix.md)),
  which goes beyond the current RFC scope but is directly relevant evidence.

Issue tracking: reddb-io/toon#104 (this feedback package), spec program
reddb-io/toon#93, grammar freeze reddb-io/toon#99.

## Results summary

Measured with the frozen-grammar prototype
(`scripts/wire_efficiency_s3_prototype.mjs --check`) over
`tests/wire-efficiency/corpora.json`, tokens via local `o200k_base`:

| Scenario | Wire | Bytes vs minified JSON | Tokens vs minified JSON |
| --- | --- | ---: | ---: |
| tagged-300 (rows + small label lists) | primitive-array column | −48.4% | −29.5% |
| tree3-100 (3-level nested object arrays) | child tables | −48.5% | −30.4% |
| matrix-150x8 (uniform numeric matrix) | matrix-as-child-table | +0.2% | +6.4% |

Baseline context: on both winning corpora, plain TOON v3.3 is *worse* than
minified JSON (the array-valued fields force the table into expanded list
form), so the extension recovers a shape where TOON currently loses.

The matrix form is expressible in the same grammar but is a token **loss**
against minified JSON — we document it as not recommended for a token win.

An LLM-readability sanity check (single-pass structural retrieval over
control / truncated / extra-row / width-mismatch documents) showed the proposed
wires preserve TOON's self-checking property: all three failure modes remain
detectable, where minified JSON misses all three. Details in the
[child-tables proposal](proposals/child-tables-and-matrix.md#llm-readability-sanity-check).

## Trade-offs we found

- **Per-cell list length is not declared** (primitive-array columns). The
  parent `[N]` row count and `{fields}` width remain the guardrails; a
  semantically missing final list item inside a well-formed cell is not
  independently count-checked. We raised this at grammar freeze and accepted
  it deliberately — the failure mode is narrow and the token win is large —
  but an official RFC should make this trade-off explicit.
- **Child tables carry a stronger guardrail**: the parent cell stores the
  child row count, so truncation and surplus rows are local parse errors.
  If spec#49 evolves toward object-array fields, per-row counts are worth
  keeping for that reason.
- **Delimiter choice is data-dependent**: tab beats comma exactly when it
  removes more quoting than it costs (comma-heavy free-text columns). There is
  no universal constant; guidance should say "measure your payload".

## Draft comment for spec#48 (delimiter choice)

> We implemented the three-delimiter mechanism (comma default, tab, pipe)
> end-to-end in a v3.3-compatible dialect — JS + Rust libraries and a CLI, both
> implementations passing 100% of the official conformance corpus — and wanted
> to report back some implementation experience:
>
> - Declaring the active delimiter in each array header (`[N|]`, `[N\t]`, with
>   the field list using the same delimiter) kept every header locally legible;
>   we found "absence of a symbol always means comma, no inheritance from the
>   parent header" to be the rule that made nested tables easy to read and
>   parse.
> - Because delimiter selection never changes the decoded value, it turned out
>   to be a zero-compatibility-risk knob: every output remains valid v3.3.
> - The efficiency win is data-dependent, not constant. Tab beats comma exactly
>   when it removes more per-cell quoting than it costs — i.e. comma-heavy
>   free-text columns. Our guidance ended up as: comma by default; tab when
>   cells routinely contain commas; pipe for human-facing tables. We'd suggest
>   the RFC frame the choice as "measure your payload" rather than promising a
>   universal saving.
>
> Design history and worked examples:
> https://github.com/reddb-io/toon/blob/main/docs/proposals/delimiter-choice.md
> — happy to share more details or test cases if useful.

## Draft comment for spec#49 (array-valued fields)

> We implemented this ahead of the spec in a v3.3-compatible dialect (JS +
> Rust + CLI, 100% official conformance corpus on both implementations) and
> measured it with `o200k_base`, so here are concrete numbers and one design
> trade-off worth surfacing.
>
> **Grammar we froze.** In an otherwise-tabular array, `field[;]` declares a
> primitive-list cell whose in-cell sub-delimiter is `;` (always distinct from
> the active row delimiter); empty list = empty cell; items follow ordinary
> scalar quoting, and anything unrepresentable falls back to plain v3.3 for the
> whole table. Encode is opt-in, decode always-on, ineligible shapes fall back
> deterministically.
>
> **Measured result.** On a 300-row tagged-record corpus (rows + a small list
> of string labels — the shape that motivates this RFC), the wire is **−48.4%
> bytes and −29.5% tokens vs minified JSON**. Notably, plain TOON v3.3 is
> *worse* than minified JSON on that corpus (the array field forces expanded
> list form), so this recovers a shape where TOON currently loses.
>
> **The trade-off to make explicit.** A primitive-list cell does not declare
> its own item count. The table's `[N]` row count and `{fields}` width still
> hold, but a semantically missing final list item inside a well-formed cell is
> not independently count-checked. We accepted this consciously at our grammar
> freeze (narrow failure mode, large win), but if this RFC advances we'd
> recommend the spec state that trade-off — TOON's self-checking guardrails are
> a big part of its value.
>
> **One step further, as evidence.** We also shipped recursive *child tables*
> for object-array fields (`items{sku,qty}` in the header; the parent cell
> stores the child row count; child rows indent below). That keeps a stronger
> guardrail (every child count is checked, so truncation is a local parse
> error) and measures **−48.5% bytes / −30.4% tokens vs minified JSON** on a
> 3-level tree corpus. A uniform numeric matrix is expressible in the same
> grammar but is a token *loss* (+6.4%) against minified JSON — worth an honest
> caveat in any spec text.
>
> Full design history, grammar, corpus and error taxonomy:
> https://github.com/reddb-io/toon/blob/main/docs/proposals/primitive-array-columns.md
> and
> https://github.com/reddb-io/toon/blob/main/docs/proposals/child-tables-and-matrix.md
> — happy to contribute test fixtures or wording if that helps the RFC move.

## Links

- Proposals: [delimiter-choice](proposals/delimiter-choice.md),
  [primitive-array-columns](proposals/primitive-array-columns.md),
  [child-tables-and-matrix](proposals/child-tables-and-matrix.md)
- Dialect spec: [`toon-reddb-spec.md`](toon-reddb-spec.md)
- Benchmark harness: `scripts/wire_efficiency_s3_prototype.mjs` +
  `tests/wire-efficiency/corpora.json`
- Upstream RFCs: [spec#48](https://github.com/toon-format/spec/issues/48),
  [spec#49](https://github.com/toon-format/spec/issues/49)
