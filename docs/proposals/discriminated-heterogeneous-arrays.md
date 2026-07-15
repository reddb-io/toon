# Proposal — Discriminated / heterogeneous arrays

**Stage:** 1 — measured proposal
**Status:** prototype only; recommendation is do not implement either candidate as-is.
**Spec section:** *(none; not graduated)*
**Upstream RFC:** *(none)*
**Repo issues / PRs:** [#140](https://github.com/reddb-io/toon/issues/140)

## Motivation

TOON v3.3 is strongest when an array of objects can be written as one table.
Discriminated object arrays and nested heterogeneous arrays often miss that
path: rows share a prefix of common fields, then diverge by `type`, `kind`,
`action`, or a variant-specific object key. Current TOON therefore falls back
to expanded list form and repeats common keys on every row. In the current
benchmarks this is visible in `benchmarks/datasets/tagged-records` and
`benchmarks/datasets/nested-heterogeneous`.

The design question was whether a narrow wire extension can recover that
amortization without touching the normative library, JS package, CLI, or
grammar before the measurement is known.

## Design / grammar

The prototype measures two hypothetical wires. Both are line-oriented sketches,
not frozen grammar. Both encode only object arrays with at least one scalar
common prefix field and decode back to the exact original JSON value.

### Candidate C — common-prefix table plus payload

Candidate C preserves row order directly. For each eligible array, the encoder
keeps the scalar common prefix as columns and emits one trailing payload cell
containing the row-specific suffix object. No rows are reordered, so lossless
round trip does not need an order vector.

Sketch:

```toon
@array $C0 path=events n=4 prefix=type,id,created_at,actor
"issue_opened"  "evt_001"  "2026-07-15T09:00:00Z"  "alice"  {"issue":{...}}
"comment_added" "evt_002"  "2026-07-15T09:07:00Z"  "bob"    {"comment":{...}}
@end
```

It decodes as:

```json
{"events":[{"type":"issue_opened","id":"evt_001","created_at":"2026-07-15T09:00:00Z","actor":"alice","issue":{...}}]}
```

Eligibility in the prototype:

- array items must all be objects;
- at least one leading key must be present on every row and have scalar values;
- the payload is the remaining object suffix in original key order;
- nested arrays are measured independently when they satisfy the same rule.

This candidate is intentionally conservative: it never reorders records, but it
still pays a payload cell cost for nested or variant data.

### Candidate B — grouped subtables plus order vector

Candidate B groups rows by a discriminator and emits an order vector so the
decoder can reconstruct the original interleaving. When the order is a repeated
cycle, the vector is RLE-style:

```toon
@array $B0 path=events discr=type omit=type order=cycle(issue_opened,comment_added,check_completed,deployment)*25
@group issue_opened n=25
{"id":"evt_001","created_at":"2026-07-15T09:00:00Z","actor":"alice","issue":{...}}
@group comment_added n=25
{"id":"evt_002","created_at":"2026-07-15T09:07:00Z","actor":"bob","comment":{...}}
@end
```

For top-level discriminators such as `type`, the grouped rows omit that field
and restore it from the group label. For nested discriminators such as
`properties.kind.const`, the prototype can group by the nested value, but cannot
omit it without a more invasive path-aware cell grammar. This is the main cost
of Candidate B on nested heterogeneous data.

## How to test it

The prototype is intentionally outside normative code:

```sh
node scripts/discriminated_heterogeneous_arrays_prototype.mjs --check
```

It reads:

- `benchmarks/datasets/tagged-records/activity-events-small.json`
- `benchmarks/datasets/tagged-records/activity-events-large.json`
- `benchmarks/datasets/nested-heterogeneous/json-schema-event-small.json`
- `benchmarks/datasets/nested-heterogeneous/json-schema-event-large.json`

For each file it emits Candidate C and Candidate B wires, decodes each wire,
and asserts `JSON.stringify(decoded) === JSON.stringify(original)`. The same
run reports UTF-8 bytes and `o200k_base` token counts versus minified JSON,
canonical TOON v3.3, and the best current TOON output from the existing
extension options.

## Measured numbers

Measured by `scripts/discriminated_heterogeneous_arrays_prototype.mjs --check`
using `js-tiktoken` `o200k_base`.

| Corpus | Arrays | JSON bytes | TOON v3.3 bytes | Best current bytes | C bytes | B bytes | C bytes vs JSON | B bytes vs JSON | JSON tokens | TOON v3.3 tokens | Best current tokens | C tokens | B tokens | C tokens vs JSON | B tokens vs JSON |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| tagged-records/small | 1 | 697 | 759 | 759 | 654 | 839 | -6.2% | +20.4% | 216 | 258 | 258 | 248 | 271 | +14.8% | +25.5% |
| tagged-records/large | 1 | 20,360 | 22,191 | 22,191 | 16,491 | 17,906 | -19.0% | -12.1% | 6,386 | 7,632 | 7,632 | 6,302 | 5,864 | -1.3% | -8.2% |
| nested-heterogeneous/small | 2 | 1,621 | 1,966 | 1,966 | 1,737 | 1,805 | +7.2% | +11.4% | 447 | 509 | 509 | 502 | 521 | +12.3% | +16.6% |
| nested-heterogeneous/large | 2 | 28,378 | 29,963 | 29,963 | 28,105 | 29,500 | -1.0% | +4.0% | 8,459 | 9,405 | 9,405 | 8,592 | 9,157 | +1.6% | +8.3% |

Round trip was lossless for every candidate wire in the table.

## Why it is a good decision

Recommendation: **do not implement Candidate C or Candidate B as-is.**

Candidate B is the only token winner on the largest tagged-records case
(-8.2% vs minified JSON, -23.2% vs current TOON), but it loses on the small
tagged-records file and both nested-heterogeneous files. The order vector is
cheap when the discriminator sequence is cyclic, but it becomes real overhead
when the interleaving is short, irregular, or not strongly repeated. It also
adds a second reconstruction mechanism that every decoder and conformance test
would need to guard.

Candidate C is simpler and has better byte behavior, but it does not solve the
token problem. It still loses to JSON tokens on three of the four required
measurements and only barely beats JSON tokens on tagged-records/large (-1.3%).
The payload cell keeps the wire lossless and order-preserving, but most nested
heterogeneous content remains JSON-like inside that payload.

The useful follow-up is narrower than either candidate: a future Stage 0 idea
could target repeated, cyclic tagged-record arrays only, where Candidate B's
grouping wins clearly. That should be a separate proposal with stronger
eligibility rules and a more compact order grammar.

## Stage transitions

- **Stage 0 — idea:** issue [#140](https://github.com/reddb-io/toon/issues/140).
- **Stage 1 — measured proposal:** this document and
  `scripts/discriminated_heterogeneous_arrays_prototype.mjs`.
- **Stage 2 — frozen grammar:** not advanced.
- **Stage 3 — implemented opt-in:** not advanced.
- **Stage 4 — graduated:** not advanced.

## Links

- Prototype: `scripts/discriminated_heterogeneous_arrays_prototype.mjs`
- Corpora: `benchmarks/datasets/tagged-records`,
  `benchmarks/datasets/nested-heterogeneous`
- Repo issue: https://github.com/reddb-io/toon/issues/140
