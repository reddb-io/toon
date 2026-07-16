# TOON proposals

**tl;dr.** This directory is the design history of the reddb-io TOON dialect,
organized like the [TC39 process](https://tc39.es/process-document/): each
extension or robustness feature is a *proposal* that advances through numbered
stages until it graduates into the normative spec. Every graduated proposal
links to its section in [`toon-reddb-spec.md`](../toon-reddb-spec.md), and that
section links back here, so you can always trace a wire feature to the
motivation, measurements, and decisions that produced it.

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
| **4** | Graduated | Documented as normative in [`toon-reddb-spec.md`](../toon-reddb-spec.md); covered by the shared conformance corpus. |

## Proposals

| Proposal | Stage | Status | Spec section | Upstream RFC | Repo issues / PRs |
| --- | :---: | --- | --- | --- | --- |
| [Nested tabular headers](nested-tabular-headers.md) | 4 | Graduated | [Extension 1](../toon-reddb-spec.md#extension-1--nested-tabular-headers) | [spec#46](https://github.com/toon-format/spec/issues/46) | — |
| [Keyed-map collapse](keyed-map-collapse.md) | 4 | Graduated | [Extension 2](../toon-reddb-spec.md#extension-2--keyed-map-collapse) | [spec#57](https://github.com/toon-format/spec/issues/57) | — |
| [Delimiter choice](delimiter-choice.md) | 4 | Graduated | [Delimiter choice](../toon-reddb-spec.md#delimiter-choice) | [spec#48](https://github.com/toon-format/spec/issues/48) | — |
| [Depth guard](depth-guard.md) | 4 | Graduated | [Depth guard](../toon-reddb-spec.md#depth-guard) | — | — |
| [detectTruncation](detect-truncation.md) | 4 | Graduated | [detectTruncation](../toon-reddb-spec.md#detecttruncation--structured-completeness-reports) | — | — |
| [Primitive-array columns](primitive-array-columns.md) | 4 | Graduated (landed via #100/#101) | [Extension 3](../toon-reddb-spec.md#extension-3--primitive-array-columns) | [spec#49](https://github.com/toon-format/spec/issues/49) | [#97](https://github.com/reddb-io/toon/issues/97), [#99](https://github.com/reddb-io/toon/issues/99), [#100](https://github.com/reddb-io/toon/pull/100), [#101](https://github.com/reddb-io/toon/pull/101) |
| [Child tables + matrix](child-tables-and-matrix.md) | 4 | Graduated (landed via #102/#103) | [Extension 4](../toon-reddb-spec.md#extension-4--object-array-columns) | — | [#99](https://github.com/reddb-io/toon/issues/99), [#102](https://github.com/reddb-io/toon/pull/102), [#103](https://github.com/reddb-io/toon/pull/103) |
| [Discriminated / heterogeneous arrays](discriminated-heterogeneous-arrays.md) | 1 | Measured; do not implement as-is | — | — | [#140](https://github.com/reddb-io/toon/issues/140) |
| [Cyclic discriminated arrays](cyclic-discriminated-arrays.md) | 4 | Graduated on the tabular wire | [Extension 5](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays) | — | [#142](https://github.com/reddb-io/toon/issues/142), [#150](https://github.com/reddb-io/toon/issues/150), [#151](https://github.com/reddb-io/toon/issues/151), [#168](https://github.com/reddb-io/toon/issues/168), [#172](https://github.com/reddb-io/toon/issues/172), [#174](https://github.com/reddb-io/toon/issues/174) |

## Adding a proposal

1. Copy [`template.md`](template.md) to `docs/proposals/<kebab-case-name>.md`.
2. Fill every section — motivation, design/grammar, how to test, measured
   numbers, why it is a good decision, stage transitions, links.
3. Add a row to the table above at the stage it currently sits.
4. When it graduates, add a normative section to
   [`toon-reddb-spec.md`](../toon-reddb-spec.md), a `> **Proposal history:**`
   backlink in that section, and flip the stage here to **4**.
