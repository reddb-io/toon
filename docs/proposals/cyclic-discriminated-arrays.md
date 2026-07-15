# Proposal — Cyclic discriminated arrays

**Stage:** 4 — graduated (landed via [#150](https://github.com/reddb-io/toon/issues/150) / [#151](https://github.com/reddb-io/toon/issues/151))
**Status:** graduated into `toon-reddb-spec.md` as Extension 5; decode always-on, encode opt-in, fail-closed.
**Spec section:** [Extension 5 — Cyclic discriminated arrays](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays)
**Upstream RFC:** *(none)*
**Repo issues / PRs:** [#142](https://github.com/reddb-io/toon/issues/142), [#150](https://github.com/reddb-io/toon/issues/150), [#151](https://github.com/reddb-io/toon/issues/151)

## Motivation

The broader [discriminated / heterogeneous arrays](discriminated-heterogeneous-arrays.md)
proposal found that general-purpose heterogeneous-array wires were not worth
implementing. The only clear token win was Candidate B on large cyclic
tagged-record streams: group records by discriminator and carry enough order
metadata to reconstruct the original interleaving.

This proposal narrows that idea to strongly cyclic discriminated object arrays.
The target shape is an event stream where `type`, `kind`, or a similar
discriminator repeats in a stable cycle such as:

```text
open,comment,check,deploy,open,comment,check,deploy,...
```

Irregular, short, partially cyclic, or random sequences are not a target. They
must remain in the lossless TOON v3.3 fallback path.

## Design / grammar

This proposal refined Candidate B from the previous proposal with stricter
deterministic eligibility and a compact order grammar. The shipped normative
syntax is [Extension 5](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays).

### Eligibility

An array is eligible only when all rules pass:

- every item is an object;
- every row has the same scalar string discriminator key, currently detected
  from `type`, `kind`, or `event`;
- the discriminator sequence has a repeated cycle of length 2 through 8;
- the cycle repeats at least three full times;
- the shipped grammar admits only complete cycles, with no tail form;
- the compact order expression must be at most 40% of the raw per-row
  discriminator list;
- controls that are non-cyclic, partially cyclic below the repeat threshold, or
  random are ineligible and use a byte-identical JSON/TOON fallback in the
  prototype.

The strict gate is intentional. This extension should not become a second
general heterogeneous-array format.

### Order grammar

The order vector is encoded once as the cycle plus repetition count:

```toon
order=cycle(open,comment,check,deploy)*60
```

The shipped grammar deliberately rejects the prototype's pressure-test tail
form. Every emitted or accepted order expands to exactly the declared row count.

### Grouped payload tables

Rows are grouped by discriminator. The discriminator itself is omitted from
payload rows and restored from the group label. Scalar common-prefix fields
after the discriminator are factored into one common table in original row
order, so grouped payload rows only carry variant-specific suffixes.

Sketch:

```toon
@toon-cyclic-discriminated-array/1
@root {"events":"$C0"}
@array $C0 discr=type n=240 common=id,ts,actor order=cycle(open,comment,check,deploy)*60
@common
"evt_00000" "2026-07-15T08:00:00Z" "user_2"
"evt_00001" "2026-07-15T08:01:00Z" "user_5"
@group open n=60
{"issue":"ISS-1000","priority":"low"}
@group comment n=60
{"comment":"comment 1","mentions":2}
@group check n=60
{"check":"build","duration_ms":5844}
@group deploy n=60
{"env":"prod","sha":"81a7d43c"}
@end
```

The decoder reconstructs rows by expanding the order grammar, reading the next
payload from each group cursor, and merging:

```js
{ [discriminator]: label, ...commonRow, ...groupPayload }
```

## How to test it

- JS: `serialize(value, { cyclicDiscriminatedArrays: true })`
- Rust: `to_toon_with_options(EncodeOptions { cyclic_discriminated_arrays: true, .. })`
- `tq`: `--cyclic-discriminated-arrays`

Decoding is always-on. The shared wire-efficiency corpus covers decode values,
encode bytes, and malformed fail-closed cases across both implementations.

The original prototype remains useful as design evidence:

```sh
node scripts/cyclic_discriminated_arrays_prototype.mjs --check
```

It generates deterministic corpora with a seeded LCG and no network:

- cyclic event streams with cycle lengths 2, 3, 4, and 5;
- record counts from 24 through 500;
- minimal and richer field mixes;
- non-cyclic controls: irregular sequence, short partial cycle, and shuffled
  random sequence.

For every eligible prototype wire it decodes and asserts:

```js
JSON.stringify(decoded) === JSON.stringify(original)
```

For every ineligible control it asserts that the fallback wire is exactly the
minified JSON value, proving the gate excludes non-cyclic inputs without data
loss. The report compares UTF-8 bytes and `o200k_base` tokens against minified
JSON, canonical TOON v3.3, and the best current TOON-family output available
through existing encoder options.

## Measured numbers

Prototype measurements came from
`scripts/cyclic_discriminated_arrays_prototype.mjs --check` using `js-tiktoken`
`o200k_base`. The graduation decision uses the shipped re-measurement in
[`benchmarks/results/2026-07-15-token-efficiency.md`](../../benchmarks/results/2026-07-15-token-efficiency.md),
which measured the implemented extension through the JS package and Rust crate.

The shipped Rust implementation is the best TOON-family format for the cyclic
representative shape, with a median **10.2% token reduction versus minified
JSON** across the four cyclic datasets. The amortization curve remains positive
from the smallest complete cyclic corpus: 24 records save 4.2% tokens; 90, 240,
and 500 records save 9.8%, 10.7%, and 11.3% respectively.

Historical prototype table:

| Corpus | Eligible | Rows | Cycle | Repeats | JSON bytes | TOON v3.3 bytes | Best current bytes | Cyclic bytes | Cyclic bytes vs JSON | JSON tokens | TOON v3.3 tokens | Best current tokens | Cyclic tokens | Cyclic tokens vs JSON |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cycle2-24-minimal | yes | 24 | 2 | 12 | 2,387 | 2,531 | 2,531 | 1,940 | -18.7% | 905 | 1,084 | 1,084 | 867 | -4.2% |
| cycle3-90-rich | yes | 90 | 3 | 30 | 10,361 | 11,021 | 11,021 | 7,556 | -27.1% | 3,905 | 4,684 | 4,684 | 3,524 | -9.8% |
| cycle4-240-rich | yes | 240 | 4 | 60 | 27,385 | 29,074 | 29,074 | 19,567 | -28.5% | 10,500 | 12,577 | 12,577 | 9,378 | -10.7% |
| cycle5-500-rich | yes | 500 | 5 | 100 | 55,386 | 58,895 | 58,895 | 38,840 | -29.9% | 21,435 | 25,645 | 25,645 | 19,022 | -11.3% |
| control-non-cyclic-irregular | no | 12 | — | — | 1,366 | 1,449 | 1,449 | 1,366 | 0.0% | 525 | 625 | 625 | 525 | 0.0% |
| control-partial-cycle | no | 9 | — | — | 1,043 | 1,106 | 1,106 | 1,043 | 0.0% | 397 | 473 | 473 | 397 | 0.0% |
| control-random-types | no | 80 | — | — | 9,155 | 9,715 | 9,715 | 9,155 | 0.0% | 3,509 | 4,193 | 4,193 | 3,509 | 0.0% |

Round trip was lossless for every eligible cyclic wire. Every non-cyclic
control was rejected by eligibility and used the lossless fallback.

## Why it is a good decision

Decision: **the cyclic discriminated-array shape is implemented and graduated,
but only with the deterministic eligibility gate and compact RLE order grammar
above.**

The measured token gain starts at the smallest complete cyclic corpus in this
prototype: 24 records over a 2-item cycle wins by 4.2% tokens and 18.7% bytes
versus minified JSON. The gain becomes materially stronger once there are at
least about 90 records over a 3-item cycle: 9.8% tokens and 27.1% bytes versus
JSON, and 24.8% tokens versus canonical TOON v3.3. Larger cyclic streams plateau
around 10% to 11% token savings versus JSON and roughly 26% versus current TOON.

The frontier is therefore narrow:

- implement when the discriminator order is strongly cyclic, repeats at least
  three times, and compresses the order grammar below the 40% threshold;
- do not implement for short event lists, one-off heterogeneous arrays,
  irregular streams, random ordering, or partial cycles below the repeat gate;
- reject tail support; no real corpus justified its grammar complexity.

This is a better recommendation than the previous general Candidate B because
the eligibility rule removes the losing cases instead of making every decoder
pay for them. It preserves the key invariant that ineligible inputs fall back to
existing lossless TOON v3.3 behavior.

## Trade-offs

The upside is clear on strongly cyclic event streams: repeated discriminator
labels and repeated common-prefix fields move out of every row, while the order
grammar stays tiny.

The cost is another reconstruction mechanism. Implementations would need
careful validation of group counts, common-row counts, order expansion length,
and missing groups. A malformed wire must fail closed; it cannot silently drop
or duplicate events.

Factoring common prefix fields into a separate table keeps grouped payload rows
small, but it means decoders merge three sources per row: discriminator label,
common row, and group payload. That is acceptable only because eligibility is
narrow and deterministic.

## Stage transitions

- **Stage 0 — idea:** issue [#142](https://github.com/reddb-io/toon/issues/142).
- **Stage 1 — measured proposal:** this document and
  `scripts/cyclic_discriminated_arrays_prototype.mjs`.
- **Stage 2 — frozen grammar:** complete-cycle `cycle(...)*N` order grammar;
  tail support rejected.
- **Stage 3 — implemented opt-in:** `cyclicDiscriminatedArrays` /
  `cyclic_discriminated_arrays` / `--cyclic-discriminated-arrays`.
- **Stage 4 — graduated:** [Extension 5](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays).

## Links

- Spec section: [Extension 5 — Cyclic discriminated arrays](../toon-reddb-spec.md#extension-5--cyclic-discriminated-arrays)
- Prototype: `scripts/cyclic_discriminated_arrays_prototype.mjs`
- Benchmark report: [`2026-07-15-token-efficiency.md`](../../benchmarks/results/2026-07-15-token-efficiency.md)
- Prior proposal: `docs/proposals/discriminated-heterogeneous-arrays.md`
- Repo issues: [#142](https://github.com/reddb-io/toon/issues/142), [#150](https://github.com/reddb-io/toon/issues/150), [#151](https://github.com/reddb-io/toon/issues/151)
