# TOON proposals

**tl;dr.** This directory is design history, organized like the
[TC39 process](https://tc39.es/process-document/): each extension or robustness
feature is a *proposal* that advanced through numbered stages. It now spans two
kinds of outcome, both measured against the **TOON v4.1** baseline
([ADR 0005](../../.red/adr/0005-rebase-on-spec-v4-1-with-event-based-decoders.md)):

- **Absorbed into the official spec.** Some proposals were adopted upstream and
  are now part of the official TOON specification at v4.1 (nested tabular
  headers via spec#46, keyed-map collapse via spec#57). The official syntax and
  semantics govern; the proposal here is retained as design history only, with
  no independent normative weight.
- **Surviving reddb-io opt-in extensions.** The rest remain live reddb-io
  extensions, re-expressed on the v4.1 base: decode always-on and fail-closed,
  encode opt-in, defined normatively in
  [`toon-reddb-spec.md`](../toon-reddb-spec.md). Each links to its reddb
  extension section, and that section links back here.

The normative behavior lives in the specs — [official companion](../toon-official-spec.md),
[reddb flavor](../toon-reddb-spec.md), [TOONL streaming](../toonl-reddb-spec.md).
These proposals are the *why* and the *how we got here*; the spec is the *what*.

## Stages

Mapped onto the TC39 process:

| Stage | Name | Meaning |
| ---: | --- | --- |
| **0** | Idea | An informal sketch — a problem worth solving, no committed design. |
| **1** | Measured proposal | A concrete design with a prototype and first token/byte measurements. |
| **2** | Frozen grammar | The wire grammar is locked; implementation slices may begin. JS and Rust never design independently. |
| **3** | Implemented opt-in | Shipped behind an encoder opt-in flag; decoding is always-on and fail-closed. |
| **4** | Graduated | Either **absorbed into the official spec** (now governed by the official TOON v4.1 specification) or shipped as a **live reddb-io extension**, documented as normative in [`toon-reddb-spec.md`](../toon-reddb-spec.md); covered by the shared conformance corpus. |

## Proposals

| Proposal | Stage | Status | Spec section | Upstream RFC | Repo issues / PRs |
| --- | :---: | --- | --- | --- | --- |
| [Nested tabular headers](nested-tabular-headers.md) | 4 | Absorbed into official spec v4.1 | [official spec](../toon-official-spec.md) | [spec#46](https://github.com/toon-format/spec/issues/46) | — |
| [Keyed-map collapse](keyed-map-collapse.md) | 4 | Absorbed into official spec v4.1 | [official spec](../toon-official-spec.md) | [spec#57](https://github.com/toon-format/spec/issues/57) | — |
| [Delimiter choice](delimiter-choice.md) | 4 | Graduated | [Delimiter choice](../toon-reddb-spec.md#delimiter-choice) | — | — |
| [Depth guard](depth-guard.md) | 4 | Graduated | [Depth guard](../toon-reddb-spec.md#depth-guard) | — | — |
| [detectTruncation](detect-truncation.md) | 4 | Graduated | [detectTruncation](../toon-reddb-spec.md#detecttruncation--structured-completeness-reports) | — | — |
| [Primitive-array columns](primitive-array-columns.md) | 4 | Graduated (landed via #100/#101) | [Extension 3](../toon-reddb-spec.md#extension-3--primitive-array-columns) | — | [#97](https://github.com/reddb-io/toon/issues/97), [#99](https://github.com/reddb-io/toon/issues/99), [#100](https://github.com/reddb-io/toon/pull/100), [#101](https://github.com/reddb-io/toon/pull/101) |
| [Child tables + matrix](child-tables-and-matrix.md) | 4 | Graduated (landed via #102/#103) | [Extension 4](../toon-reddb-spec.md#extension-4--object-array-columns) | — | [#99](https://github.com/reddb-io/toon/issues/99), [#102](https://github.com/reddb-io/toon/pull/102), [#103](https://github.com/reddb-io/toon/pull/103) |
| [Discriminated / heterogeneous arrays](discriminated-heterogeneous-arrays.md) | 1 | Measured; do not implement as-is | — | — | [#140](https://github.com/reddb-io/toon/issues/140) |
| [Cyclic discriminated arrays](cyclic-discriminated-arrays.md) | 4 | Graduated on the tabular wire | [Extension 5](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays) | — | [#142](https://github.com/reddb-io/toon/issues/142), [#150](https://github.com/reddb-io/toon/issues/150), [#151](https://github.com/reddb-io/toon/issues/151), [#168](https://github.com/reddb-io/toon/issues/168), [#172](https://github.com/reddb-io/toon/issues/172), [#174](https://github.com/reddb-io/toon/issues/174) |

## Adding a proposal

1. Copy [`template.md`](template.md) to `docs/proposals/<kebab-case-name>.md`.
2. Fill every section — motivation, design/grammar, how to test, measured
   numbers, why it is a good decision, stage transitions, links.
3. Add a row to the table above at the stage it currently sits.
4. When it graduates, flip the stage here to **4** and record the outcome:
   - **Absorbed into the official spec** — the mechanism is adopted upstream and
     governed by the official TOON specification. Mark the Status column
     "Absorbed into official spec vN", add a design-history banner at the top of
     the proposal, and note it carries no independent normative weight.
   - **Live reddb-io extension** — add a normative reddb extension section to
     [`toon-reddb-spec.md`](../toon-reddb-spec.md), a `> **Proposal history:**`
     backlink in that section, and mark the Status column as a live reddb-io
     opt-in extension on the current TOON baseline.
