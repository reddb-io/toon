# Proposal — Cyclic discriminated arrays

**Stage:** 1 — measured redesign.
**Status:** the shipped Extension 5 `@toon-cyclic-discriminated-array/1` wire is
implemented today, but this proposal now treats it as a design mistake to be
superseded. The replacement wire below is genuine TOON: nested metadata plus
tabular sub-tables, with no directive envelope, no embedded JSON object payload
lines, and no `$` references.
**Spec section:** [Extension 5 — Cyclic discriminated arrays](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays) currently documents the shipped wire and is pending re-implementation.
**Repo issues / PRs:** [#142](https://github.com/reddb-io/toon/issues/142), [#150](https://github.com/reddb-io/toon/issues/150), [#151](https://github.com/reddb-io/toon/issues/151), [#168](https://github.com/reddb-io/toon/issues/168)

## Motivation

The previous implementation found a real token win for strongly cyclic tagged
records, but the wire was not TOON. It used an out-of-band directive envelope:

```text
@toon-cyclic-discriminated-array/1
@root {"events":"$C0"}
@array $C0 ...
@group ...
{"payload":"as JSON object literals"}
```

That shape betrayed the format. It made the payload rows JSON again and moved
the structure into ad hoc directives. The measured redesign keeps the same
eligibility and round-trip contract, but expresses the data as normal TOON
objects and tabular arrays.

## Eligibility

The new wire keeps the same narrow gate:

- every item is an object;
- every row has the same scalar string discriminator key, detected from `type`,
  `kind`, or `event`;
- the discriminator sequence has a strong repeated cycle of length 2 through 8;
- the cycle repeats at least three full times;
- no tail form is emitted or accepted;
- the compact `cycle(...)*N` order expression remains below the size threshold;
- ineligible values fall back losslessly to canonical/default output.

The encode/decode invariants remain unchanged: encode is opt-in, decode is
always-on after re-implementation, strict v3 fails closed, round-trip is
lossless, and default output remains canonical byte-identical TOON v3.3.

## Genuine TOON wire

The replacement is an ordinary nested TOON object:

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

The `order`, `discriminator`, and `rows` fields are scalar TOON fields. The
common fields stay in original row order in `common[N|]{...}:`. Each
discriminator value owns a real tabular sub-table, so group payload keys appear
once in the table header instead of being repeated per row. Nested payloads are
flattened into tabular paths in the prototype, including fixed array positions
and `.length` guard columns, so the wire still contains scalar cells rather than
JSON objects.

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
benchmark. It demonstrates how nested payloads stay tabular instead of becoming
JSON object lines:

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

## Prototype

Run:

```sh
node scripts/cyclic_discriminated_arrays_prototype.mjs --check
node scripts/cyclic_discriminated_arrays_prototype.mjs --check --samples
```

The prototype now measures:

- the real tagged-record corpora in `benchmarks/datasets/tagged-records`;
- deterministic synthetic cyclic corpora with cycle lengths 2, 3, 4, and 5;
- non-cyclic controls for irregular order, partial cycle, and shuffled labels;
- minified JSON, canonical TOON v3.3, best current TOON-family output, the
  shipped directive wire, and the genuine TOON redesign;
- `o200k_base` tokens and UTF-8 bytes;
- round-trip losslessness by decoding both cyclic wires back to byte-identical
  minified JSON.

`--check` also asserts that every eligible genuine wire has no `@toon-`
directive, no `$C0`-style reference, and no JSON object literal payload lines.

## Measured numbers

Measured with `node scripts/cyclic_discriminated_arrays_prototype.mjs --check`
using `js-tiktoken` / `o200k_base`:

| Corpus | Eligible | Rows | Cycle | Repeats | JSON bytes | TOON bytes | Shipped bytes | Genuine bytes | Genuine bytes vs shipped | JSON tokens | TOON tokens | Shipped tokens | Genuine tokens | Genuine tokens vs shipped |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| tagged-records-small | no | 4 | — | — | 697 | 759 | 697 | 697 | 0.0% | 216 | 258 | 216 | 216 | 0.0% |
| tagged-records-large | yes | 120 | 4 | 30 | 20,360 | 22,191 | 14,826 | 10,917 | -26.4% | 6,386 | 7,632 | 5,634 | 4,939 | -12.3% |
| cycle2-24-minimal | yes | 24 | 2 | 12 | 2,387 | 2,531 | 1,940 | 1,585 | -18.3% | 905 | 1,084 | 867 | 800 | -7.7% |
| cycle3-90-rich | yes | 90 | 3 | 30 | 10,361 | 11,021 | 7,556 | 6,232 | -17.5% | 3,905 | 4,684 | 3,524 | 3,252 | -7.7% |
| cycle4-240-rich | yes | 240 | 4 | 60 | 27,385 | 29,074 | 19,567 | 16,569 | -15.3% | 10,500 | 12,577 | 9,378 | 8,720 | -7.0% |
| cycle5-500-rich | yes | 500 | 5 | 100 | 55,386 | 58,895 | 38,840 | 33,546 | -13.6% | 21,435 | 25,645 | 19,022 | 17,826 | -6.3% |
| control-non-cyclic-irregular | no | 12 | — | — | 1,366 | 1,449 | 1,366 | 1,366 | 0.0% | 525 | 625 | 525 | 525 | 0.0% |
| control-partial-cycle | no | 9 | — | — | 1,043 | 1,106 | 1,043 | 1,043 | 0.0% | 397 | 473 | 397 | 397 | 0.0% |
| control-random-types | no | 80 | — | — | 9,155 | 9,715 | 9,155 | 9,155 | 0.0% | 3,509 | 4,193 | 3,509 | 3,509 | 0.0% |

The result is stronger than merely "as efficient as the shipped wire": every
eligible measured corpus improved. The genuine TOON redesign saves **6.3% to
12.3% tokens** and **13.6% to 26.4% bytes** versus the shipped directive wire,
while preserving the same lossless fallback for ineligible cases.

## Recommendation

Replace the shipped directive wire with the genuine TOON wire above before
calling this extension graduated. The redesign is structurally honest TOON,
keeps the deterministic eligibility boundary, round-trips losslessly in the
prototype, and is more token-efficient than the current shipped format on every
eligible measured corpus.

No normative code should change until the bytes above are accepted. The next
implementation slice should update the JS package, Rust crate, `tq`, and the
normative spec together so the public extension surface moves atomically from
the `@` directive wire to the tabular sub-table wire.

## Links

- Prototype: `scripts/cyclic_discriminated_arrays_prototype.mjs`
- Current normative spec section: [Extension 5 — Cyclic discriminated arrays](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays)
- Benchmark datasets: `benchmarks/datasets/tagged-records/`
- Prior proposal: `docs/proposals/discriminated-heterogeneous-arrays.md`
