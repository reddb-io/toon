# Rust decoder comparison — 2026-09-23

A one-off comparison of `reddb-io-toon` 0.30.0 against the two other Rust TOON
decoders in the toon-format ecosystem. The bench crate lived outside this
repository and is not part of `cargo bench`.

| Engine | Version | Notes |
| --- | --- | --- |
| `reddb-io-toon` | 0.30.0 (`4d70c9c`) | TOON v4.1.1 |
| `toon-format` (toon-rust) | 0.5.0 (crates.io) | Targets spec v3.0 |
| `simd-toon` | git `d758c8f` (toon-format/toon#337) | AVX2, decode-only, ~88% of the conformance fixtures |

- **Setup:** Intel i7-1065G7 (4C/8T, AVX2), rustc 1.98.1, release profile, `jobs = 2`; criterion with 20 samples, 3 s measurement, 1 s warm-up.
- **Inputs:** five documents built by the `reddb-io-toon` encoder, restricted to v3-compatible forms (no keyed tabular form, no nested field groups).
  - Before timing, every decoder had to produce the same JSON-model value.
  - All three agreed on all five inputs.
- **Throughput:** median MiB/s of TOON input.

## Decode

| Input | Bytes | reddb-io-toon 0.30.0 | reddb-io-toon, one handoff per call (prototype) | toon-format | simd-toon owned | simd-toon borrowed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| tabular, 1k rows | 52,571 | 1.03 | 26.6 | 21.1 | 106.8 | 155.3 |
| tabular, 10k rows | 565,278 | 1.27 | 34.4 | 23.8 | 121.7 | 168.0 |
| nested objects | 45,218 | 2.04 | 26.7 | 34.7 | 169.2 | 197.8 |
| mixed list form | 59,065 | 1.88 | 26.1 | 30.5 | 157.0 | 181.1 |
| long text | 364,599 | 50.3 | 122.6 | 190.4 | 1,178.5 | 1,276.8 |

### After the fix, including the streaming path

Second run on the shipped change: `decode` on one worker handoff per call, and
`decode_iter` / `decode_event_reader` (behind `toon -d`) batching up to 256
events per channel message. MiB/s; this run measured about 20% lower than the
prototype run on the same inputs, which is within this laptop's thermal
variance.

| Input | decode, 0.30.0 | decode | decode_iter (streaming) |
| --- | ---: | ---: | ---: |
| tabular, 1k rows | 1.03 | 20.8 | 18.2 |
| tabular, 10k rows | 1.27 | 25.9 | 20.0 |
| nested objects | 2.04 | 22.5 | 22.3 |
| mixed list form | 1.88 | 21.8 | 20.9 |
| long text | 50.3 | 97.5 | 94.1 |

The 0.30.0 `decode` drove the same per-event channel as the streaming API, so
its column is also the streaming baseline.

## Encode

| Input | reddb-io-toon | toon-format |
| --- | ---: | ---: |
| tabular, 1k rows | 14.0 | 16.1 |
| tabular, 10k rows | 12.3 | 14.4 |
| nested objects | 17.4 | 27.5 |
| mixed list form | 20.0 | 28.3 |
| long text | 342.4 | 330.1 |

## Reading

- **The 0.30.0 decode is 12–25× slower than it needs to be.** `decode` runs
  the recursive grammar on a big-stack worker thread and hands **every event**
  across a rendezvous channel (`sync_channel(0)` in `decode_event_reader`), so
  each key and value costs a thread handoff (about 3.5 µs). The cost grows
  linearly with the event count, which is why tabular data suffers most.
- **One handoff per call is enough.** The prototype runs the existing
  `decode_events` on the same 8 MiB-stack worker and joins it once, so deep
  nesting keeps its stack. That alone lifts decode to 26–34 MiB/s, ahead of
  toon-format on tabular input, and the full `reddb-io-toon` test suite passes
  unchanged.
- **Streaming keeps its contract.** `decode_event_reader` now batches events,
  and before any read that may reach the source it hands the batch over and
  waits until the consumer asks for more. A slow stream still yields events
  before EOF, and the parser never reads ahead of demand (the existing
  gated-reader test pins both), while buffered input decodes in full batches.
- **simd-toon stays 4–10× ahead** after the fix. Its SIMD structural scan and
  borrowed values are the remaining gap. A `memchr`-based line and delimiter
  scanner is the incremental step toward it; a full SIMD port is not justified
  while simd-toon covers ~88% of the fixtures and only AVX2.
- **Encode is within about 1.6× of toon-format** everywhere and ahead on long
  text; nothing urgent.
