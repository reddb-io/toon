# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed

- **TOONL v0.2 support** is now implemented across the Rust crate, JS package,
  and `tq` CLI: resumable readers, continuation headers, header-preserving
  trim, tagged-row multiplexing, and per-lane/interleaved close transforms are
  covered by the shared v0.2 conformance corpus.

### Added

- **TOONL v0.2 specification** (`docs/toonl-v0.2.md`): a normative, requirements-only
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

  Boundaries: v0.2 is not yet implemented in the Rust crate or the JS package
  (encoders/decoders remain v0.1); the TOON v3.3 document spec is unchanged; no v0.1
  semantics change. Implementation is a follow-up Spec.
