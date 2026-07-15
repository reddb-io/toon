# tests/ — Evidence Map

This directory holds the complete test evidence for the TOON implementation.
Every corpus file is dual-runner (byte-identical between the Rust and JS harnesses).

## Directory layout

```
tests/
  corpus/              — shared machine-readable corpora (JSON)
    toon/              — encode/decode fixtures (encode/, decode/ subdirs)
    toonl/             — TOONL stream fixtures (v0_1.json, v0_2.json)
    truncation.json    — truncation-detection corpus
    json-limits.json   — JSON boundary and adversarial round-trip corpus
    wire-efficiency/   — byte-size efficiency corpora (corpora.json + per-mode files)
  runners/
    rust/              — Rust integration test entry points (.rs)
      toon/            — crate `reddb-io-toon` runners + ledgers
      tq/              — crate `reddb-io-tq` runners
  golden/
    tq/                — CLI golden test cases (31 scenarios)
  README.md            — this file
```

JS runners live in `packages/toon/test/` (vitest/node:test convention) and
point to the same `tests/corpus/` files.

## Corpus → what it proves → who consumes it → provenance

| Corpus | What it proves | Rust runner | JS runner | Provenance |
|---|---|---|---|---|
| `corpus/toon/` | encode/decode round-trip parity with local extensions | `runners/rust/toon/spec_conformance.rs` | `packages/toon/test/conformance.test.mjs` | ours |
| `vendor/toon-spec/tests/fixtures` | official spec conformance (official TOON v3.x fixtures) | `runners/rust/toon/spec_conformance.rs` | `packages/toon/test/conformance.test.mjs` | upstream spec ([toon-format/spec](https://github.com/toon-format/spec)) |
| `corpus/toonl/` | TOONL v0.1 / v0.2 stream encode+decode | `runners/rust/toon/spec_conformance.rs` | `packages/toon/test/conformance.test.mjs` | ours |
| `corpus/truncation.json` | truncation-detection structured report | _(api.rs covers the API; no dedicated Rust runner)_ | `packages/toon/test/toon.test.mjs` | ours |
| `corpus/json-limits.json` | numbers, unicode strings, depth limits, adversarial round-trip | `runners/rust/toon/json_limits.rs` | `packages/toon/test/json-limits.test.mjs` | ours |
| `corpus/wire-efficiency/` | encoded byte sizes for tabular and map-collapse modes | `runners/rust/toon/wire_efficiency.rs` | `packages/toon/test/wire-efficiency.test.mjs` | ours |
| `golden/tq/` | CLI argument handling, filter pipeline, output modes | `runners/rust/tq/golden.rs` | — | ours |

## Expected-failure ledgers

| Ledger | Scope |
|---|---|
| `runners/rust/toon/expected-failures.txt` | Official spec fixtures (vendor/toon-spec) that this implementation does not yet satisfy. Ratchet — entries may only be removed. |
| `runners/rust/toon/vendor-toon-expected-failures.txt` | Behavioral divergences from the reference TypeScript implementation (vendor/toon). Currently empty. |

## Vendor submodules

| Path | Remote | What we consume |
|---|---|---|
| `vendor/toon-spec` | [toon-format/spec](https://github.com/toon-format/spec) | `tests/fixtures/` — official decode/encode test vectors |
| `vendor/toon` | [toon-format/toon](https://github.com/toon-format/toon) | Reference implementation (TypeScript). No standalone JSON vectors — test data is embedded in TypeScript files. We track behavioral divergences in `vendor-toon-expected-failures.txt`. We do **not** run their suite in our CI. |

Initialize both with:
```
git submodule update --init
```
