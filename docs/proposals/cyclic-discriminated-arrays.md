# Proposal — Cyclic discriminated arrays

**Stage:** 4 — graduated.
**Status:** graduated on the tabular TOON wire. The normative behavior is now
defined in [Extension 5 — Cyclic discriminated arrays](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays).
**Spec section:** [Extension 5 — Cyclic discriminated arrays](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays).
**Repo issues / PRs:** [#142](https://github.com/reddb-io/toon/issues/142), [#150](https://github.com/reddb-io/toon/issues/150), [#151](https://github.com/reddb-io/toon/issues/151), [#168](https://github.com/reddb-io/toon/issues/168), [#172](https://github.com/reddb-io/toon/issues/172), [#174](https://github.com/reddb-io/toon/issues/174)

## Motivation

Strongly cyclic tagged records repeat a discriminator such as `type`, `kind`, or
`event` in a stable order. Plain TOON v3.3 repeats that discriminator value in
every row and expands heterogeneous payload fields. The graduated wire keeps the
same narrow eligibility boundary, but expresses the compression as normal TOON:
scalar metadata, a common-field table, and one tabular sub-table per
discriminator label.

This design intentionally avoids a separate envelope language. A strict v3.3
reader sees an ordinary nested TOON object. An extension-aware reader uses the
metadata to reconstruct the original array.

## Eligibility

The encoder MAY emit the cyclic form only when all of these hold:

- the root value is a non-empty object whose values are arrays;
- every item in each candidate array is an object;
- every row has the same scalar string discriminator key, detected from `type`,
  `kind`, then `event`;
- the discriminator sequence is a complete repeated cycle of unique labels with
  cycle length 2 through 8;
- the cycle repeats at least three full times;
- no tail form is emitted or accepted;
- the compact `cycle(...)*N` order expression remains below the size threshold;
- common fields are the contiguous primitive fields immediately after the
  discriminator and present as primitive values in every row; and
- every discriminator-specific payload sub-table has a uniform scalar leaf shape
  after dotted-path flattening. Nested objects are eligible only when all rows
  for that discriminator share the same nested object shape; primitive arrays are
  eligible only when they have a uniform fixed length across all rows for that
  discriminator.

Ineligible values fall back losslessly to canonical/default output. Encoding is
opt-in; default output remains byte-identical TOON v3.3.

## Tabular TOON wire

The graduated wire is an ordinary nested TOON object:

```toon
events:
  order: cycle(open,comment)*12
  discriminator: type
  rows: 24
  common[24|]{id|ts}:
    "evt_00000"|"2026-07-15T08:00:00Z"
    "evt_00001"|"2026-07-15T08:01:00Z"
  open[12|]{issue|priority}:
    "ISS-1000"|"low"
    "ISS-1002"|"high"
  comment[12|]{comment|mentions}:
    "comment 1"|0
    "comment 3"|1
```

The `order`, `discriminator`, and `rows` fields are scalar TOON fields.
`common[N|]{...}:` stores fields shared by every row in original row order. Each
discriminator value owns a real tabular sub-table, so payload keys appear once
in the table header instead of being repeated per row.

Nested payloads are flattened into dotted tabular paths. A nested object path
uses field names such as `issue.number` and `issue.title`. A fixed primitive
array uses a `.length` guard plus numeric element paths, such as
`labels.length`, `labels.0`, and `labels.1`.

### Complete sample — synthetic cycle2

```toon
events:
  order: cycle(open,comment)*12
  discriminator: type
  rows: 24
  common[24|]{id|ts}:
    "evt_00000"|"2026-07-15T08:00:00Z"
    "evt_00001"|"2026-07-15T08:01:00Z"
    "evt_00002"|"2026-07-15T08:02:00Z"
    "evt_00003"|"2026-07-15T08:03:00Z"
    "evt_00004"|"2026-07-15T08:04:00Z"
    "evt_00005"|"2026-07-15T08:05:00Z"
    "evt_00006"|"2026-07-15T08:06:00Z"
    "evt_00007"|"2026-07-15T08:07:00Z"
    "evt_00008"|"2026-07-15T08:08:00Z"
    "evt_00009"|"2026-07-15T08:09:00Z"
    "evt_00010"|"2026-07-15T08:10:00Z"
    "evt_00011"|"2026-07-15T08:11:00Z"
    "evt_00012"|"2026-07-15T08:12:00Z"
    "evt_00013"|"2026-07-15T08:13:00Z"
    "evt_00014"|"2026-07-15T08:14:00Z"
    "evt_00015"|"2026-07-15T08:15:00Z"
    "evt_00016"|"2026-07-15T08:16:00Z"
    "evt_00017"|"2026-07-15T08:17:00Z"
    "evt_00018"|"2026-07-15T08:18:00Z"
    "evt_00019"|"2026-07-15T08:19:00Z"
    "evt_00020"|"2026-07-15T08:20:00Z"
    "evt_00021"|"2026-07-15T08:21:00Z"
    "evt_00022"|"2026-07-15T08:22:00Z"
    "evt_00023"|"2026-07-15T08:23:00Z"
  open[12|]{issue|priority}:
    "ISS-1000"|"low"
    "ISS-1002"|"high"
    "ISS-1004"|"medium"
    "ISS-1006"|"low"
    "ISS-1008"|"high"
    "ISS-1010"|"medium"
    "ISS-1012"|"low"
    "ISS-1014"|"high"
    "ISS-1016"|"medium"
    "ISS-1018"|"low"
    "ISS-1020"|"high"
    "ISS-1022"|"medium"
  comment[12|]{comment|mentions}:
    "comment 1"|0
    "comment 3"|1
    "comment 5"|1
    "comment 7"|0
    "comment 9"|2
    "comment 11"|3
    "comment 13"|0
    "comment 15"|2
    "comment 17"|0
    "comment 19"|0
    "comment 21"|3
    "comment 23"|1
```

### Complete sample — benchmark group with nested payload

This is the complete `issue_opened` sub-table from the tagged-records large
benchmark. It demonstrates how nested payloads stay tabular:

```toon
issue_opened[30|]{issue.number|issue.title|issue.labels.length|issue.labels.0|issue.labels.1}:
  131|"Build realistic token corpus"|2|"benchmark"|"tokens"
  135|"Regenerate token report"|2|"benchmark"|"tokens"
  139|"Validate benchmark harness"|2|"benchmark"|"tokens"
  143|"Build realistic token corpus"|2|"benchmark"|"tokens"
  147|"Regenerate token report"|2|"benchmark"|"tokens"
  151|"Validate benchmark harness"|2|"benchmark"|"tokens"
  155|"Build realistic token corpus"|2|"benchmark"|"tokens"
  159|"Regenerate token report"|2|"benchmark"|"tokens"
  163|"Validate benchmark harness"|2|"benchmark"|"tokens"
  167|"Build realistic token corpus"|2|"benchmark"|"tokens"
  171|"Regenerate token report"|2|"benchmark"|"tokens"
  175|"Validate benchmark harness"|2|"benchmark"|"tokens"
  179|"Build realistic token corpus"|2|"benchmark"|"tokens"
  183|"Regenerate token report"|2|"benchmark"|"tokens"
  187|"Validate benchmark harness"|2|"benchmark"|"tokens"
  191|"Build realistic token corpus"|2|"benchmark"|"tokens"
  195|"Regenerate token report"|2|"benchmark"|"tokens"
  199|"Validate benchmark harness"|2|"benchmark"|"tokens"
  203|"Build realistic token corpus"|2|"benchmark"|"tokens"
  207|"Regenerate token report"|2|"benchmark"|"tokens"
  211|"Validate benchmark harness"|2|"benchmark"|"tokens"
  215|"Build realistic token corpus"|2|"benchmark"|"tokens"
  219|"Regenerate token report"|2|"benchmark"|"tokens"
  223|"Validate benchmark harness"|2|"benchmark"|"tokens"
  227|"Build realistic token corpus"|2|"benchmark"|"tokens"
  231|"Regenerate token report"|2|"benchmark"|"tokens"
  235|"Validate benchmark harness"|2|"benchmark"|"tokens"
  239|"Build realistic token corpus"|2|"benchmark"|"tokens"
  243|"Regenerate token report"|2|"benchmark"|"tokens"
  247|"Validate benchmark harness"|2|"benchmark"|"tokens"
```

## Strict-v3 read behavior

Because the wire is ordinary TOON, a strict TOON v3.3 decoder reads it literally:
an object with scalar `order`, `discriminator`, and `rows` fields plus tabular
fields named `common` and by discriminator labels. It does not reconstruct the
source array. Consumers that require the reconstructed array must use an
extension-aware decoder.

## Measured numbers

Measured with `pnpm benchmark:tokens` and recorded in
[`benchmarks/results/2026-07-15-token-efficiency.md`](../../benchmarks/results/2026-07-15-token-efficiency.md):

| Corpus | Rows | JSON tokens | Cyclic tokens | Tokens vs JSON |
| --- | ---: | ---: | ---: | ---: |
| cycle2-24-minimal | 24 | 905 | 716 | -20.9% |
| cycle3-90-rich | 90 | 3,905 | 2,862 | -26.7% |
| cycle4-240-rich | 240 | 10,500 | 7,640 | -27.2% |
| cycle5-500-rich | 500 | 21,435 | 15,676 | -26.9% |

The representative cyclic shape measured a median **26.8% token reduction
versus minified JSON**, with crossover first observed at 24 records. The JS and
Rust cyclic implementations produced the same measured cyclic token counts in
that report.

## Graduation

The proposal is graduated on the tabular wire because the design now satisfies
the dialect's extension contract:

- encoding remains opt-in;
- default output remains canonical TOON v3.3;
- extension-aware decode round-trips losslessly for eligible cyclic arrays;
- ineligible values fall back losslessly; and
- strict v3.3 readers see a literal grouped object instead of a silently wrong
  reconstructed array.

## Links

- Normative spec: [Extension 5 — Cyclic discriminated arrays](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays)
- Benchmark report: [`benchmarks/results/2026-07-15-token-efficiency.md`](../../benchmarks/results/2026-07-15-token-efficiency.md)
- Benchmark datasets: `benchmarks/datasets/tagged-records/`
- Prior proposal: [`Discriminated / heterogeneous arrays`](discriminated-heterogeneous-arrays.md)
