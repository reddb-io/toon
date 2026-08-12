# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Every slice that changes the `tq` query language adds an entry here — a new
construct, a new builtin, a changed diagnostic, or a changed divergence. The
[tq language reference](docs/tq-language.md) describes the surface as it stands
now; this file is how it got there.

## [Unreleased]

### Changed

- **Breaking:** `tq` now follows jq when iterating with `.[]`: objects emit
  their values in field order, while `null` and scalar inputs raise an error.
  Use `.[]?` to suppress those iteration errors. The former array-only
  behavior was ledgered as `divergence-iteration-on-object`; that ledger row
  and its corpus case were retired with the change.
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

- **Breaking: the pre-v4 engine and every API that reached it are gone.** TOON
  v4.1 is now the only codec in both languages.
  - TypeScript: the `@reddb-io/toon/legacy` subpath and the modules behind it
    (the old parser, header reader, serializer, and option resolver) are
    deleted. `decode` and `encode` are the whole decode and encode surface.
  - Rust: `parse_legacy`, `parse_legacy_with_options`, `to_legacy_toon*`,
    `try_to_legacy_toon*`, `LegacyParseOptions`, `LegacyEncodeOptions`,
    `detect_truncation_legacy`, and `detect_truncation_legacy_with_options` are
    deleted, along with the parser, writer, and header modules behind them.
  - `ParseOptions::expand_paths` is deleted. Dotted-key expansion only ever ran
    on the removed parser, so the option had nothing left behind it.
  - `Array::Tabular` and `TabularArray` are deleted. The removed parser was
    their only producer; the v4.1 event decoder materialises every array as
    `Array::List`. The `test-hooks` feature and its row-decode counter go with
    them.
  - Observable differences for callers moving off the old API: the canonical
    encoder emits no trailing newline, normalises a non-finite number to `null`
    the way `JSON.stringify` does, refuses to emit an unpaired surrogate, spells
    the keyed-table header `key[n:]{fields}:` rather than `key{fields}:`, and
    reports the v4.1 error checklist's own messages.
  - A test gate greps shipped source in both languages so these symbols cannot
    return.
- **Path expansion** (`expandPaths`) and **key folding** (`keyFolding`) were
  removed from the spec; the encoder never folds. See the
  [v4.1 migration notes](docs/migration-v4.md) for before/after decode behavior.

### Fixed

- **TOONL v0.2 support** is now implemented across the Rust crate, JS package,
  and `tq` CLI: resumable readers, continuation headers, header-preserving
  trim, tagged-row multiplexing, and per-lane/interleaved close transforms are
  covered by the shared v0.2 conformance corpus.

### Added

- **A jq-style query language in `tq`**, built slice by slice and pinned by the
  vendored jq 1.7.1 parity corpus in `tests/corpus/tq/parity/`. The
  [tq language reference](docs/tq-language.md) is the normative description,
  including the precedence ladder, the builtin catalog with a
  supported/deferred/never status for every name, and the
  "Where tq differs from jq" table drawn from the divergence ledger in
  [docs/tq-jq-parity.md](docs/tq-jq-parity.md).
  - **Parity infrastructure**: the `.cases` corpus format, the hermetic replay
    against vendored expectations, the optional validator that replays against
    jq only when it is exactly 1.7.1, and the divergence ledger.
  - **Operators**: `and`, `or`, `not`, the alternative `//`, and `%`, all placed
    on jq's precedence ladder.
  - **Control flow**: `if`/`elif`/`else`/`end`, `try`/`catch`, the `?` postfix,
    `empty`, and `error`.
  - **Indexing**: generalized `.[e]`, slices, and iteration.
  - **User-defined functions**: `def` with filter and `$`-valued parameters,
    recursion, closures, shadowing, and a bounded recursion depth that reports
    `exceeded the maximum filter recursion depth` instead of exhausting the
    stack.
  - **The path layer**: `path`, `paths`, `leaf_paths`, `getpath`, `setpath`,
    `delpaths`, `del`, `pick`, recursive descent (`..`, `recurse`), `tostream`,
    and `fromstream`. Reads stay lazy over the codec's accessors; only writes
    materialise the tabular array they touch.
  - **The assignment family**: `=`, `|=`, `+=`, `-=`, `*=`, `/=`, `%=`, and
    `//=`, all lowered onto `setpath`, with jq's non-associativity, its
    right-hand-side evaluation rules, and `|= empty` as deletion.
  - **Strings, formats, and JSON conversions**: `"\(…)"` interpolation,
    `@text`, `@json`, `@csv`, `@tsv`, `@base64`, `@base64d`, `@uri`, `@html`,
    `@sh`, the `@format "…"` prefix form, and `tostring`, `tonumber`, `tojson`,
    `fromjson`.
  - **Builtin sweeps**: types and selectors, array and stream ops, object ops,
    math, regex and strings, a UTC-only time subset, and the runtime builtins
    `debug`, `stderr`, `halt`, `halt_error`, and stream-aware `input`/`inputs`.
  - **CLI surface for the language**: `-n`, `-R`/`--raw-input`, `--arg`, and
    `--argjson`, which put `$name` and `$ARGS` in scope, plus `-j`, `-S`, and
    `-e`.

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
