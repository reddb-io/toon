# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

- **Breaking:** `tq` now follows jq when iterating with `.[]`: objects emit
  their values in field order, while `null` and scalar inputs raise an error.
  Use `.[]?` to suppress those iteration errors.
- **Rebased the baseline on the official TOON spec v4.1.** The former v3.3
  baseline is retired; the `vendor/toon` / `vendor/toon-spec` submodules are
  pinned at the v4.1.1 checkpoint, and the decoders are rebuilt as event-based
  streaming decoders targeting the v4.1 rules (see ADR 0005). The default
  encoder output is now canonical TOON v4.1.
- **Two mechanisms were absorbed by the official spec at v4.1** and are no
  longer reddb-io inventions: nested tabular headers (upstream RFC spec#46,
  "nested field groups") and keyed-map collapse (upstream RFC spec#57, "keyed
  tabular form"). The remaining opt-in extensions were re-expressed on the v4.1
  base and keep their decode-always-on / encode-opt-in / fail-closed contract.
- **Strict mode was hardened** to the v4.1 authoritative error checklist,
  including a tightened numeric grammar (leading-zero, `+`-prefixed, bare
  fractional, and non-finite tokens decode as strings).

### Removed

- **Path expansion** (`expandPaths`) and **key folding** (`keyFolding`) were
  removed from the spec. `expandPaths` is retained only as a non-normative
  legacy shim (default off); the encoder never folds. See the
  [v4.1 migration notes](docs/migration-v4.md) for before/after decode behavior.

### Fixed

- **TOONL v0.2 support** is now implemented across the Rust crate, JS package,
  and `tq` CLI: resumable readers, continuation headers, header-preserving
  trim, tagged-row multiplexing, and per-lane/interleaved close transforms are
  covered by the shared v0.2 conformance corpus.

### Added

- **TOONL v0.2 specification** (now unified into `docs/toonl-reddb-spec.md`): a normative, requirements-only
  spec that formally closes the red-skills requirements R1–R4. It promotes
  suffix-closure, concatenation closure, and the header-on-open discipline to
  first-class data-model guarantees, and builds on them:
  - **R1 — resumable readers**: a `{byteOffset, activeHeaderLine, rowsSinceHeader}`
    cursor convention with a resume guarantee, invalidation conditions (truncation
    and anchor mismatch), and an OPTIONAL `[~]{fields}:` continuation header for
    long-lived single-segment streams.
  - **R2 — header-preserving trim**: a row-counted keep-last-N algorithm built on
    suffix-closure, the drop-or-recount trailer rule, atomic tmp+rename writes, and
    the `tq trim --keep-last N` verb contract.
  - **R3 — tagged-row multiplexing**: named schema declarations `[]<tag>{fields}:`
    and tagged rows `<tag>:...`, a bounded (≥8-lane) live-schema table, redefinition
    as rotation, untagged-row v0.1 compatibility (single-shape streams pay nothing),
    the canonical per-shape field-order requirement, and per-lane plus
    interleave-preserving close-transforms.
  - **R4 — splice non-goal**: in-place row splice is declared an explicit non-goal,
    with the side-journal (`.retry`) pattern documented as the blessed retry/re-queue
    mechanism, resting on concatenation closure + header-on-open.
  - v0.1/v0.2 compatibility and version-signaling rules, a worked example for every
    new construct, and an R1–R4 traceability map.

  Boundaries: v0.2 is implemented by the Rust crate, JS package, and `tq`; the
  base TOON document spec is unchanged by TOONL, and no v0.1 semantics change.
