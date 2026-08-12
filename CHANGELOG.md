# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

- **Breaking (Rust):** the canonical codec now owns the unsuffixed API names.
  `EncodeV4Options` is now `EncodeOptions`, `encode_v4` is `encode_with_options`,
  `encode_v4_with_replacer` is `encode_with_replacer`, `decode_value_v4` is
  `decode_with_options`, and `detect_truncation_v4` is
  `detect_truncation_with_options`. The dialect suffix existed only to
  distinguish the canonical engine from the legacy one, and there is one engine.
- **Breaking (Rust):** the compatibility-shaped option structs are gone from the
  canonical surface. The former `ParseOptions` and `EncodeOptions` are now named
  `LegacyParseOptions` and `LegacyEncodeOptions` — the legacy parser and
  serializer are their only callers — and the type aliases of those names are
  removed. `Document::parse_with_options` and `Value::parse_with_options` now
  take `&DecodeOptions`; `to_toon_with_options`, `try_to_toon_with_options`, and
  `detect_truncation_with_options` now take the canonical `EncodeOptions` /
  `&DecodeOptions`. No public entry point silently converts between two option
  shapes any more.
- **Breaking (Rust):** `Document::parse` and `Value::parse_with_options` no
  longer default `cyclic_discriminated_arrays` to `true`. They now behave
  exactly like `decode` / `decode_with_options`, which default it to `false`;
  pass `DecodeOptions { cyclic_discriminated_arrays: true, ..Default::default() }`
  to keep the previous reconstruction.
- **Breaking (Rust):** `EncodeOptions` no longer carries `nested_tabular_headers`
  or `keyed_map_collapse`. Both forms graduated into official v4.1 syntax and
  were already unconditional on the canonical encoder; the fields were no-ops.
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
