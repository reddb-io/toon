# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Every slice that changes the `tq` query language adds an entry here — a new
construct, a new builtin, a changed diagnostic, or a changed divergence. The
[tq language reference](docs/tq-language.md) describes the surface as it stands
now; this file is how it got there.

## [Unreleased]

### Changed

- **Breaking (toon-rpc, Rust):** the Rust client correlates calls by ID.
  `Client::duplex` runs one receive task over a `DuplexTransport`, and
  `Client::request_response` settles each call from its own response. Calls
  take a timeout, a dropped call future cancels its call, and `close` or the
  stream ending rejects every pending call exactly once. Invalid, unknown-ID
  and duplicate-ID responses reach an `on_diagnostic` callback. The shared
  corpus now runs its client cases on this client. `ClientTransport` and the
  unused `Transport` trait are gone. TCP, Unix-socket and stdio transports use
  the §8.1 length-prefixed framing (`FramedTransport`, `serve_framed`) instead
  of blank-line delimiters, which broke on multi-line documents, and the
  servers bind an address the caller chooses.

- **Breaking (toon-rpc, Rust):** HTTP and WebSocket follow §8. `HttpServer`
  and `WsServer` bind an address the caller chooses (HTTP was fixed to
  `0.0.0.0:8080`). HTTP answers a notification with `204 No Content`, sends
  every error as TOON, and refuses a body over the limit with `413`; the new
  `HttpTransport` is a request/response client. WebSocket no longer sends an
  empty message for a notification, caps message and frame sizes, and answers
  in the kind of message the request used.

- **Breaking (toon-rpc, Rust):** SSE follows the §8.2 duplex profile. The
  event stream stays open and carries each response as one `data:` event; a
  POST is only acknowledged (`202`). Both legs name a client-chosen `session`
  query parameter, and closing the stream ends the session. `SseTransport` is
  the new client. The old registry, whose stream closed after one event and
  answered in the POST body, is gone. Long polling stays unpublished (spec §9
  defers it): its unauthenticated `/notify` route is removed, events are pushed
  in-process only, and the waiter table is bounded and cleaned up.

- **toon-rpc limits and graceful shutdown.** TypeScript and Rust share one set
  of limits (`Limits`, spec §8.3): frame, message and event size, HTTP body,
  batch length, pending calls, connections, receive queue, idle time and
  shutdown grace. Going past one is a defined error, a refused request or
  call, or a closed connection, never a dropped document. Every Rust server
  gains `with_limits` and `serve_with_shutdown`, which stops accepting, lets
  each connection answer the document in hand, ends open SSE streams and
  aborts what is left after the grace period.

- **toon-rpc TypeScript servers.** `@reddb-io/toon-rpc/serve` adds Node
  servers for every transport, with no new dependencies: `serveTcp` and
  `serveStdio` (§8.1 framing), `createHttpHandler` and `createSseHandler`
  (`node:http` listeners; SSE per §8.2) and `attachWebSocket` (any `ws`-shaped
  socket). They answer each connection's documents in order under the shared
  limits and close gracefully. The TypeScript side could only be a client
  before, so a Rust client had nothing to talk to.

- **Breaking (toon-rpc):** mixed dialects are handled per request. On a
  connection where the peer interleaves JSON-RPC and TOON-RPC,
  `dualDialectStream` answers each request in the dialect it arrived in (the
  connection-wide latch only applies to this side's own messages). The Rust
  `MultiRpc` now detects the dialect as the TypeScript one does (media type,
  then a real JSON parse instead of an 80-byte sniff) and validates JSON-RPC
  entries through the TOON-RPC core, so an Invalid Request carries `id: null`,
  `params: null` is invalid instead of `[]`, and a fractional or boolean `id`
  is refused instead of becoming `0`. In both, a malformed body opening with `{`
  gets a JSON-RPC Parse error, and the JSON path honors the batch limit. A new
  shared corpus, `tests/corpus/toon-rpc/multi.json`, holds both to it.

- **Breaking (MCP):** `@reddb-io/toon-rpc-mcp` and `reddb-io-toon-rpc-mcp`
  now implement the Model Context Protocol as published, pinned to the
  2025-06-18 schema: JSON-RPC 2.0 over newline-delimited JSON on stdio, the
  `initialize` lifecycle, `ping`, and the tools, resources and prompts features
  for whichever of them a service provides. The invented protocol is gone
  (`server/discover`, `resultType`, `ttlMs`, `cacheScope`, `items` envelopes,
  the fictional version `2026-07-28`, TOON on the wire, and the Rust HTTP
  endpoint). TOON remains available as an optional encoding of text content
  (`toonContent` / `toon_content`, `CallToolResult.toon`). Both implementations
  replay the shared transcript `tests/corpus/mcp/transcript.json`.

- **Breaking (toon-rpc codegen and CLI):** generated code compiles and runs.
  The IDL lists method params as an object in declaration order, and names
  are validated. The generator emits the declared types, a service trait or
  interface, a `register_*` function that reads params by name or by position,
  and a typed client, in both languages and in a deterministic order. It no
  longer emits `todo!()`, closures that could not compile, or TypeScript that
  never read its params. The calculator's generated code is committed and
  compiled, run by the examples and by the TypeScript tests, and checked
  against the IDL. The derive macro, which targeted a trait that did not
  exist, is removed. The `reddb-io-toon-rpc` CLI keeps two working commands,
  `generate` and `call <http|ws|tcp URL> <method> [TOON params]`, and reports
  the crate version instead of `0.1.0`. The examples use the generated code
  and §8.1 framing, and run as integration tests.

- **Rust `decode` builds its `Value` directly and jumps with `memchr`.** The
  value is assembled while the grammar emits, without an intermediate event
  vector, and strict mode skips the duplicate-key search it never needs
  (+12–22%). The scanners jump to the next delimiter, quote or backslash with
  `memchr`: long text gains 26%, tabular data 5–11%. Decode now runs 1.6–2.2×
  toon-format on tables and 1.6× on long text.

- **Rust decoding scans bytes instead of chars.** Delimiters, quotes and colons
  are ASCII, so the hot scanners no longer decode UTF-8 one `char` at a time.
  Cells are borrowed slices, and quoted strings copy whole runs between
  escapes. Decode throughput rises 25–46% on tabular, nested and list inputs
  and doubles on long text, ahead of toon-rust on tables and long text.

- **The TypeScript codec is faster than the upstream reference.** Object keys
  were set with `Object.defineProperty`, three times the cost of an assignment,
  which made normalization half of a tabular encode; only `__proto__` still
  needs it. Cycle tracking starts at depth 32. On the same inputs, encode goes
  from 0.70× to 1.24× the reference on a 10k-row table (1.38–1.66× elsewhere)
  and decode runs at 1.56–2.06×.

- **Rust decoding is 12–25× faster.** `decode` and the `Value` parsers ran the
  grammar on a worker thread that handed every event across a rendezvous
  channel, about 3.5 µs per key or value (tabular input decoded at about
  1 MiB/s). `decode` now joins its big-stack worker once per call, and the
  streaming `decode_iter` / `decode_event_reader` (and `toon -d`) move up to
  256 events per handoff while still yielding before EOF and never reading
  ahead of demand. See
  [the decoder comparison](benchmarks/results/2026-09-23-rust-decoder-comparison.md).

- **Breaking (Rust):** `DecodeStreamOptions` (alias `DecodeOptions`) gains the
  public fields `max_input_bytes`, `max_array_length` and `max_keys`, so a
  struct literal without `..Default::default()` no longer compiles. A limit
  error's `Display` reads `input exceeds maxInputBytes (N)`, matching the
  TypeScript message.
- **Rust numbers use the reference `Number#toString` layout.** Non-integral
  numbers are written with shortest round-trip digits, plain inside
  `[1e-6, 1e21)` and in exponent form outside it, so `5e-324` is no longer a
  330-character decimal and `1e21` is `1e+21`. Both encoders now agree byte for
  byte on every shared corpus case. Integer digits are still kept verbatim.
- **A blank line inside an array reports the blank line**, in both engines, as
  the upstream reference does; it used to report the next content line.

- **Breaking (Rust):** the one codec now owns the unsuffixed API names.
  `EncodeV4Options` is `EncodeOptions`, `encode_v4` is `encode_with_options`,
  `encode_v4_with_replacer` is `encode_with_replacer`, `decode_value_v4` is
  `decode_with_options`, and `detect_truncation_v4` is
  `detect_truncation_with_options`. The dialect suffix existed only to
  distinguish the canonical engine from the pre-v4 one, and that engine is gone.
- **Breaking (Rust):** the compatibility-shaped option structs are gone.
  `ParseOptions` is removed in favour of `DecodeOptions`, and the adapters that
  converted between the two option shapes are removed with it.
  `Document::parse_with_options` and `Value::parse_with_options` now take
  `&DecodeOptions`, while `to_toon_with_options` and `try_to_toon_with_options`
  take the encoder's own `EncodeOptions`. No public entry point silently
  converts between two option shapes any more.
- **Breaking (Rust):** `Document::parse_with_options` and
  `Value::parse_with_options` no longer default `cyclic_discriminated_arrays` to
  `true`. They now behave exactly like `decode_with_options`, which defaults it
  to `false`; pass
  `DecodeOptions { cyclic_discriminated_arrays: true, ..Default::default() }`
  to keep the previous reconstruction.
- **Breaking (Rust):** `EncodeOptions` no longer carries
  `nested_tabular_headers` or `keyed_map_collapse`. Both forms graduated into
  official v4.1 syntax and were already unconditional on the canonical encoder,
  so the fields were no-ops.
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

- **Rust numbers keep every digit.** `Value::from_json_str` and `toon -e` read
  a document holding a number with 16 or more mantissa digits through a
  lossless reader, instead of rounding it through an `f64`. Decimals with more
  precision than an `f64` are canonicalized in exact decimal arithmetic, still
  in JavaScript's layout (a token with shortest round-trip digits prints
  exactly as before), and `toon -d` writes every finite number from its
  canonical text. The weekly toon-diff now compares the Rust engines exactly,
  with no tolerated numeric class.
- **TypeScript and Rust report every decode error identically**, in the
  upstream reference's words where its tests pin them. Rust now spells out
  counts (`expected 3 tabular rows, but got 2`) through `ParseError::detail()`
  and its `Display`, while `message()` keeps the fixed category
  (`array count mismatch`). Both engines say `missing colon after key`,
  `invalid array length`, and `unterminated string: missing closing quote`
  where they said `expected key-value line`, `malformed array header length`,
  and `invalid quoted string`. The shared CLI goldens now cover these errors,
  and the upstream package suite runs with no wording skips (643/643). Branch
  on `kind` rather than message text.
- **Deep documents no longer overflow small thread stacks in Rust.**
  `detect_truncation` and `decode_events` ran the recursive grammar on the
  caller's thread, so a document nested past `max_depth` aborted the process
  from a 2 MiB thread instead of returning the depth error. Every entry point
  now runs on the decoder's own stack, raised to 16 MiB.
- **The Rust `toon -d` writes integers of any size exactly.** Integer tokens
  beyond `i64`/`u64` went through `serde_json` and came out as the nearest
  `f64` (`18446744073709551616` became `1.8446744073709552e+19`); they are now
  written digit for digit. JSON *input* is still read through `serde_json` and
  is exact only within `i64`/`u64`, which the crate README now states instead
  of claiming that larger integers survive. Found by running the toon-diff
  differential tester (toon-format/toon discussion #323) against both engines.
- **Root strings that start with U+FEFF are quoted** (toon-format/toon#339).
  The decoder strips a document-leading U+FEFF as a byte-order mark, so an
  unquoted `\uFEFF8` decoded as the number `8` and a lone `\uFEFF` as `{}`.
  Raw root strings that start with U+FEFF are rejected.
- **Token trimming removes U+0020 only** (spec §12) in both decoders. A root
  string made of NBSP, U+2028 or U+3000 decoded as `{}`, and NBSP-edged TOONL
  and extension cells lost their edges; a tab outside its delimiter role is now
  key content, as in the upstream reference.
- **Quoted commas no longer switch a tab or pipe header to comma fields.**
  `[1\t]{"comma,value"{...}}` failed to decode in both engines. Found by the
  new round-trip oracle in the differential fuzzer.
- **TypeScript `encode` normalizes sparse array holes to `null`**
  (toon-format/toon#335) instead of emitting empty cells or throwing, detects
  circular input with a `TypeError`, and stops runaway nesting with the
  `maxDepth` error instead of overflowing the stack.
- **`toon -o` writes atomically** in both front ends: output goes to a
  temporary sibling renamed into place on success, so a failed conversion no
  longer truncates an existing file.
- **TOONL v0.2 support** is now implemented across the Rust crate, JS package,
  and `tq` CLI: resumable readers, continuation headers, header-preserving
  trim, tagged-row multiplexing, and per-lane/interleaved close transforms are
  covered by the shared v0.2 conformance corpus.

### Added

- **toon-rpc release gates.** A new `RPC gates` CI job runs every
  TypeScript ↔ Rust transport cell in both directions (HTTP, WebSocket, TCP,
  SSE, stdio: `pnpm test:rpc-interop`), installs every RPC package from its
  packed tarball and imports each export, and reports line coverage per
  component against the floors in
  `scripts/rpc-coverage-floors.json`. A stable release waits for it through
  the exact-commit CI gate.
- **The VS Code extension publishes to the Marketplace and Open VSX.** A new
  `publish-vscode` release job sends the stable `.vsix` to both registries.
  Each registry is skipped with a warning while its token (`VSCE_PAT`,
  `OVSX_PAT`) is missing, and a version already published is skipped.
- **A weekly cross-implementation diff.** `.github/workflows/toon-diff.yml`
  runs toon-diff's corpus and mutation generator through every ordered pair of
  the upstream TypeScript reference, `@reddb-io/toon` and the Rust `toon` CLI,
  using the driver in `scripts/toon-diff/`. A finding fails the run, but it
  never gates a release.
- **Decode limits for untrusted input:** `maxInputBytes`, `maxArrayLength` and
  `maxKeys` in TypeScript (`max_input_bytes`, `max_array_length`, `max_keys` in
  Rust); `0` or `Infinity` means unlimited, the default.
- **Error kinds and columns.** Decode errors carry a stable `kind`
  (`syntax`, `indentation`, `length-mismatch`, `duplicate-key`, `depth-limit`,
  `input-limit`; Rust `ErrorKind` adds `Io`) and a 1-based column for
  indentation errors.
- **Typed serde API for Rust:** `to_string`, `from_str` and their
  `_with_options` variants behind the default `serde` feature, with
  `SerdeError`.
- **`toon --check`** validates input without writing output, in both front
  ends.
- **`encodeToolManifest(tools)`** renders an MCP `tools/list` result as a
  compact TOON manifest for prompts.
- **VS Code extension features:** strict-decode diagnostics, canonical
  formatting, JSON↔TOON conversion commands and a size/token status item, on
  the codec vendored into the `.vsix`.
- **Accuracy benchmark:** a `json-object-mode` structured-output baseline,
  `OPENAI_BASE_URL` for OpenAI-compatible gateways, a dry-run mode, and a
  provenance file per run.
- **Docs:** a [cheatsheet](docs/cheatsheet.md) and a guide to
  [prompting LLMs with TOON](docs/llm-prompting.md).
- **Round-trip oracle in the differential fuzzer** and property tests for root
  strings, Unicode whitespace, sparse arrays, cycles and decoder panics.
- **Drop-in `toon` converter binaries for TypeScript and Rust.** The
  `@reddb-io/toon` package and `reddb-io-toon` crate now publish dedicated
  `toon` bins that carry the pinned upstream v4.1.1 CLI contract, including
  output-file routing, explicit encode/decode modes, and verbose diagnostics.
  A shared golden corpus and the vendored upstream CLI suite exercise both
  front-ends. The existing `tq` binary remains the jq-compatible query CLI,
  where `-o` selects a format and `-e` controls jq-style exit status.
- **`tq jq-check`, a machine-readable jq-compatibility decision.** Given a
  filter and the jq options that affect evaluation, it answers whether tq can
  execute that invocation with jq-compatible observable behavior, printing one
  JSON object and exiting `0` or `1` without evaluating the filter. A positive
  decision promises that tq reproduces jq 1.7.1's exact output on every input
  jq accepts. The decision is derived from the evaluator's own capability
  registry and the argument parser's jq-option table rather than a second
  allowlist, so a builtin gains its classification the moment it is registered,
  and a `Builtin::new(…).divergent(…)` entry is refused from then on. The
  contract, the reason vocabulary, and the fixture corpus in
  `tests/corpus/tq/compat/` are described in
  [docs/tq-jq-parity.md](docs/tq-jq-parity.md).
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
