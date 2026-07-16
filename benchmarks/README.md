# Benchmarks

This directory is the canonical source for reproducible TOON efficiency and
accuracy evidence. Benchmark results belong here, not in the root README or the
published package READMEs.

## Attribution

The benchmark layout and dataset family names are adapted from the upstream
reference implementation's benchmark methodology at `vendor/toon/benchmarks`
from `toon-format/toon`. This repository does not execute that prototype code;
the harnesses here measure the shipped implementations, especially the
`@reddb-io/toon` workspace package and the `reddb-io-toon` crate surface covered
by the repository tests.

## Token efficiency

Run:

```bash
pnpm benchmark:tokens
```

The deterministic harness reads representative offline fixtures from
`benchmarks/datasets/`, organized by shape class and documented in
`benchmarks/datasets/MANIFEST.md`. It also measures this repository's corpora
from `tests/corpus/wire-efficiency/` as extension-eligibility showcase fixtures,
not as representative payload evidence.
It compares:

- minified JSON
- pretty JSON
- JSONL
- YAML
- CSV
- XML
- canonical TOON v3.3
- TOON with each shipped opt-in extension enabled independently
- TOON with all shipped opt-in extensions enabled
- TOONL versus JSONL where the payload is a stream of records

Metrics are bytes and `o200k_base` tokens counted with `gpt-tokenizer`.
Deterministic reports are committed under `benchmarks/results/`.

## Retrieval accuracy

Run:

```bash
pnpm benchmark:accuracy
```

Accuracy is intentionally deterministic after model output: each question has a
type-aware expected value, and the validator does not use an LLM judge. API keys
are read from the environment. Copy `.env.example` for the expected variables.
Without keys, the command exits gracefully with setup instructions.

## Runtime performance

The third axis. `tokens/` measures what a payload costs to *send* and
`accuracy/` what a model *does* with it; `performance/` measures what the
shipped codec costs to *run* — wall-clock time and the bytes it moves.

Both sides measure the same four operations over the same corpora, so their
reports read side by side:

| Operation | Meaning |
| --- | --- |
| `encode` | in-memory value → TOON wire |
| `decode` | TOON wire → in-memory value |
| `json_to_toon` | JSON text → TOON wire (what `tq` does) |
| `toon_to_json` | TOON wire → JSON text |

### JS (`@reddb-io/toon`)

```bash
pnpm benchmark:performance
```

Reports median ms, MiB/s and bytes per case into
`benchmarks/results/runtime-performance-js.md`. The median (not the mean) is
reported so a single GC pause cannot define the number you compare against.

### Rust (`reddb-io-toon`)

```bash
cargo bench -p reddb-io-toon                        # measure
cargo bench -p reddb-io-toon -- --save-baseline main  # record a baseline
cargo bench -p reddb-io-toon -- --baseline main       # compare against it
```

Criterion stores baselines under `target/`, which is not committable, so
`scripts/criterion_baseline_report.mjs` renders the last run into
`benchmarks/results/runtime-performance-rust.md` — that file *is* the
committed baseline:

```bash
cargo bench -p reddb-io-toon -- --save-baseline main
node scripts/criterion_baseline_report.mjs
```

### Datasets

The representative corpus in `benchmarks/datasets/` is shared with the token
report. The performance axis adds one of its own:

| Dataset | Why it exists |
| --- | --- |
| `performance/datasets/html-heavy/` | JSON whose string values are dense markup — long runs of `"`, `\`, commas, colons and brackets in a single string. The real payload class behind the GC-thrashing encode regression fixed in #194, and the worst case for the quoting path. |

It is deterministic and regenerated with
`node scripts/generate_html_heavy_dataset.mjs`. It deliberately lives *outside*
`benchmarks/datasets/`: that corpus is a shape taxonomy chosen before measuring
token efficiency (see its `MANIFEST.md` anti-cherry-pick register), and
`html-heavy` is selected for a pathological *cost* profile, not because it
represents a payload shape worth optimizing token counts for. The token
harness reads `benchmarks/datasets/` recursively, so keeping it separate is
also what keeps the token report's taxonomy intact.

### How to read the numbers

- **Timings are machine-specific.** Compare a run against another run on the
  same machine. Never compare against a number committed from someone else's
  laptop, and never publish these as a cross-implementation claim: the Rust and
  JS harnesses measure the same operations, but a Rust-vs-JS ratio from two
  different runs is noise, not evidence.
- **A committed baseline is a starting point, not a threshold.** It exists so a
  regression is visible as a *change*, not so a number becomes a target.
- **Throughput (MiB/s) travels better than time.** It normalizes away payload
  size, so it stays meaningful when a dataset changes.
- **These benchmarks are not a gate.** CI protects against *quadratic*
  regressions with loose-bound smoke tests on both sides
  (`tests/runners/rust/toon/perf_smoke.rs`,
  `packages/toon/test/perf-smoke.test.mjs`, and the multi-megabyte case in
  `packages/toon/test/html-payload.test.mjs`). Those assert orders of
  magnitude, never milliseconds, so they cannot flake on a loaded runner.
  Detailed numbers belong here; the gate only answers "did it go quadratic?".
