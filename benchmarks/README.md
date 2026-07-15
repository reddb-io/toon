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

The deterministic harness generates upstream-style `github`, `analytics`,
`orders`, and streaming `logs` datasets, then adds this repository's corpora from
`tests/corpus/wire-efficiency/` and TOONL fixtures from `tests/corpus/toonl/`.
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
