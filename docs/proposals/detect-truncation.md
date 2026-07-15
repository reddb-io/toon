# Proposal — detectTruncation

**Stage:** 4 — graduated
**Status:** graduated into `toon-reddb-spec.md`. Diagnostic API; changes no decoded value.
**Spec section:** [detectTruncation — structured completeness reports](../toon-reddb-spec.md#detecttruncation--structured-completeness-reports)
**Upstream RFC:** —
**Repo issues / PRs:** —

## Motivation

TOON is *self-checking* in a way JSON is not: `[N]` declares a row count and
`{f1,f2}` declares a field set, so a truncated or hallucinated table is a
**structural mismatch** rather than silently short data. But the only way to
learn *why* a document is incomplete used to be to catch a decode exception and
re-derive the cause from its message. Callers that need to decide "retry, extend,
or reject" on a possibly-cut-off LLM response deserve a machine-readable answer.

## Design / grammar

No wire-format change. `detectTruncation` turns TOON's self-checking property
into a **first-class diagnostic API**, exposed identically across all three
surfaces:

- **`tq check [-p toon|toonl] [FILE]`** — prints the report and exits non-zero
  when TOON guardrails prove the input is truncated.
- **Rust** — `detect_truncation_with_options(input, options)` for TOON and
  `detect_toonl_truncation(input)` for TOONL.
- **JS** — `detectTruncation(input, { format: 'toon' | 'toonl' })`.

The report fields are stable across the CLI, the crate, and the package:
`complete`, `kind`, `line`, `declared`, `actual`, and `message`. A tabular array
that declares two rows but carries one:

```json
{
  "complete": false,
  "kind": "array_length_mismatch",
  "line": 2,
  "declared": 2,
  "actual": 1,
  "message": "declared 2 rows but received 1"
}
```

This is a **diagnosis, not a decode**: it reports the line number and the
declared-vs-actual counts without throwing.

## How to test it

- `tq check FILE` on a truncated fixture exits non-zero and prints the report;
  on a complete fixture it exits zero.
- Rust / JS: call the API on control, truncated, extra-row, and width-mismatch
  fixtures and assert the `complete` / `kind` / `line` / `declared` / `actual`
  fields. The same schema must round-trip identically on both surfaces.

## Measured numbers

Nothing to measure on tokens/bytes. The relevant evidence is the LLM-readability
sanity check that motivates the guardrails this API surfaces: across control,
truncated, extra-row, and width-mismatch scenarios, TOON's explicit shape checks
let a reader detect the violation, whereas minified JSON silently misses
truncation, extra rows, and width mismatch (see the
[child tables + matrix](child-tables-and-matrix.md#llm-readability-sanity-check)
proposal for the full table). `detectTruncation` makes exactly those checks
programmatic.

## Why it is a good decision

It converts a *property TOON already has* into an *API callers can act on*,
without changing the format. The stable field schema means a caller writes the
retry/extend/reject logic once and it works against the CLI, the crate, and the
package. It costs nothing to producers and nothing to decoded values; it only
adds a way to ask "is this complete, and if not, where and by how much."

## Stage transitions

- **Stage 0–1 — idea / measured proposal:** need for machine-readable completeness on LLM output.
- **Stage 2 — frozen grammar:** stable report schema (`complete`, `kind`, `line`, `declared`, `actual`, `message`).
- **Stage 3 — implemented opt-in:** `detectTruncation` / `detect_truncation_with_options` / `tq check`, for both TOON and TOONL.
- **Stage 4 — graduated:** [detectTruncation](../toon-reddb-spec.md#detecttruncation--structured-completeness-reports).

## Links

- Spec section: [detectTruncation — structured completeness reports](../toon-reddb-spec.md#detecttruncation--structured-completeness-reports)
- Related: [Depth guard](depth-guard.md) is the companion robustness feature; the [child tables + matrix](child-tables-and-matrix.md) proposal records the LLM-readability sanity check.
