# Backward-compatibility dead-weight audit

Date: 2026-08-11  
Scope: `@reddb-io/toon`, `reddb-io-toon`, `tq`, and repository documentation  
Tracking: [Spec #314](https://github.com/reddb-io/toon/issues/314), [audit #320](https://github.com/reddb-io/toon/issues/320)

## Standard and method

A compatibility surface survives only when there is evidence of a current
consumer or a current interoperability contract. Tests that merely assert an
alias still exists are not consumer evidence. Tests that execute a pinned
upstream contract, a cross-version wire invariant, or a documented CLI behavior
are consumer evidence.

The audit searched source, tests, fixtures, generated declarations, READMEs,
specifications, proposals, migration material, and the changelog for aliases,
deprecated names, compatibility-shaped option types and fields, legacy modes,
fallback option names, and `v3`/`v4` dialect labels. Call sites were then traced
to distinguish public compatibility from ordinary parser fallbacks and ordinary
words such as query-language `else` fallbacks. Tracked files under
`packages/toon/dist/` mirror TypeScript source and are included with their source
finding rather than counted as a second surface.

The explicit legacy parser/encoder implementation is already the subject of
the one-engine program. This document does not re-audit its internal grammar,
but it does audit the public types, aliases, methods, and documentation that
keep callers attached to it.

Classifications:

- **delete now**: no active consumer; removal is independent of the legacy
  engine teardown.
- **delete in the contract slice**: approved dead weight, but removing it before
  [#319](https://github.com/reddb-io/toon/issues/319) would either strand a live
  internal path or cause two breaking API transitions.
- **genuinely load-bearing**: retained because a current contract consumes it;
  the evidence and the condition under which it can be reconsidered are stated.

## Findings

| ID | Surface | Classification | Evidence and disposition |
| --- | --- | --- | --- |
| TS-1 | Package-root `parse` and `serialize` aliases | **delete now** | `packages/toon/src/index.ts` directly aliases them to `decode` and `encode`. Repo-wide package-root imports use the canonical names; the only root `parse` import is the migration guide's before-state. `api-contract.test.mjs` proves existence, not an application consumer. Remove the exports, generated declarations, alias-only assertions, current-surface prose, and stale SVG copy in [#328](https://github.com/reddb-io/toon/issues/328). The explicit `@reddb-io/toon/legacy` subpath remains owned by #319. |
| TS-2 | `EncodeOptions.indent` and `DecodeStreamOptions.indent` fallback to `indentSize` | **genuinely load-bearing** | Both spellings and precedence are part of the pinned upstream v4.1.1 option inventory in `docs/options-parity-v4.1.1.md`; shared conformance adapters exercise both names. [#308](https://github.com/reddb-io/toon/issues/308) already owns edge-value parity. Retain until the upstream contract drops the deprecated spelling; do not mistake deprecation alone for absence of a consumer. |
| TS-3 | `strict: false` lenient decode mode | **genuinely load-bearing** | It is an upstream v4.1.1 option, not a local v3-only shim. `toon.test.mjs`, shared event/conformance fixtures, the package README, and the migration guide exercise and document last-write-wins and relaxed validation. Retain while upstream exposes the mode. |
| TS-4 | Upstream-shaped `JsonStreamEvent`, `DecodeStreamOptions`, and `ToonDecodeError` names | **genuinely load-bearing** | These names are present in the pinned upstream v4.1.1 TypeScript API and are consumed by its stream API, CLI, tests, and API reference. The local event additionally carries a source line, but preserving the upstream names is an active source-compatibility contract rather than a retired alias. `ToonError` is different: it belongs to the explicit pre-v4 codec and leaves with #319. |
| TS-5 | `@reddb-io/toon/legacy`, `ToonError`, and its legacy `parse`/`serialize`/truncation exports | **delete in the contract slice** | These are the TypeScript public boundary of the old engine. Extension and truncation paths still import its internals, so removing the subpath before #315/#317/#318 would break live behavior. #319 already requires deleting the legacy modules and API after those consumers move. |
| RUST-1 | `LegacyParseOptions`, `LegacyEncodeOptions`, `parse_legacy*`, `to_legacy_toon*`, and legacy truncation entry points | **delete in the contract slice** | These names are the public boundary of the engine being removed. Tests and extension paths still call them today, so an earlier deletion would break the prerequisite slices. #319 explicitly removes the `Legacy*` API after #315-#318. |
| RUST-2 | Compatibility-shaped `ParseOptions`/`EncodeOptions`, their adapter functions, and canonical model methods that accept them | **delete in the contract slice** | `ParseOptions` still carries legacy-only `expand_paths`; `EncodeOptions` carries the old nested/keyed switches and feeds the legacy writer. `decode_options_from_legacy` and `encode_options_from_legacy` demonstrate that the canonical API already has `DecodeOptions` and `EncodeV4Options`. Internal parser/encoder code and public model methods are current consumers, so migrate those consumers as the engine is removed, then delete the old structs rather than preserving aliases with no distinct contract. [#330](https://github.com/reddb-io/toon/issues/330) tracks this and must close with or coordinate with #319. |
| RUST-3 | Dialect-era `decode_value_v4`, `encode_v4`, `detect_truncation_v4`, and `EncodeV4Options` names | **delete in the contract slice** | They distinguish the new engine only because the old engine still occupies unsuffixed names. `tq` and Rust tests are repo-local consumers and can migrate to `decode_with_options`, `encode_with_options`, `detect_truncation_with_options`, and the final unsuffixed option type. Removing the suffix before the option types are vacated creates churn; retire it in the same breaking contract transition as RUST-1/RUST-2. The RUST-2 follow-up covers this cluster. |
| RUST-4 | `DecodeOptions = DecodeStreamOptions` | **genuinely load-bearing** | Both names have active semantic consumers: tree-decoder APIs and `tq` use `DecodeOptions`; event-reader APIs and cross-language event fixtures use `DecodeStreamOptions`. The option set is intentionally identical, so sharing one type avoids two drifting structs. Reconsider only if the tree and event option sets diverge or one public entry-point family is removed. |
| RUST-5 | Semantic aliases `DecodeError = ParseError`, `Record = Value`, and `ToonlReader = ToonlRowReader` | **genuinely load-bearing** | Each name denotes a current public role rather than an obsolete implementation: canonical decode docs return `DecodeError`, model parse methods return `ParseError`, TOONL writers accept `Record`, and README/`tq`/integration tests construct `ToonlReader`. Retain while both named API roles are public; an alias with two active role-based consumers is not retrocompat dead weight. |
| CLI-1 | `--nested-tabular-headers` and `--keyed-map-collapse` | **delete now** | `crates/tq/src/cli/args.rs` accepts both arms and executes an empty block. Canonical v4.1 always selects these forms, so the switches cannot affect output. The only invocations are two golden fixtures; README and proposal mentions describe compatibility/history rather than an active script consumer. [#329](https://github.com/reddb-io/toon/issues/329) removes flags, usage text, passthrough fixtures, and current API claims while preserving clearly marked design history. |
| CLI-2 | `--strict` / `--no-strict` | **genuinely load-bearing** | These map directly to `DecodeOptions.strict`, match the pinned upstream v4.1.1 CLI contract, and are generated by the shared `tq` corpus runner. `--no-strict` is therefore an active codec option even though some prose calls its behavior “legacy recovery.” Retain while the upstream option survives. |
| CLI-3 | Named and literal delimiter spellings (`comma`/`,`, `tab`/tab forms, `pipe`/`|`) | **genuinely load-bearing** | These are documented CLI input conveniences, not deprecated aliases. The pipe spelling is used by golden tests and all accepted values map to the same canonical delimiter bytes. The accepted superset is recorded deliberately in the v4.1.1 parity ledger. |
| TQ-1 | jq-compatible edge semantics and the `compatibility.cases` corpus | **genuinely load-bearing** | `tq` is intentionally a jq-style query tool. The parity runner executes the vendored jq 1.7.1 corpus, while `docs/tq-jq-parity.md` separately ledgers deliberate divergences. This includes `from_entries` accepting jq's `key`/`Key`/`name`/`Name` and `value`/`Value` object fields. Case names beginning `compatibility-` are active cross-tool behavior pins, not backward shims. The documented `todateiso8601`/`fromdateiso8601` aliases are deferred and therefore are not surviving API surfaces. |
| DIALECT-1 | `failClosedV3Strict`, `rejectV3Strict`, and strict-v3 fixture/proposal terminology | **genuinely load-bearing** | TypeScript and Rust conformance/wire-efficiency runners execute the field and helper to prove extension wire fails closed in an older strict reader. That is an active cross-version safety invariant, not a selectable second engine. Retain while those extension interoperability claims remain; rename only if the invariant itself is redefined. |
| TOONL-1 | v0.1 stream acceptance and v0.1/v0.2 version language | **genuinely load-bearing** | `docs/toonl-reddb-spec.md` normatively defines v0.2 as a strict superset whose readers must accept v0.1 unchanged. Both language conformance runners execute `tests/corpus/toonl/v0_1.json` and the v0.1 rejection cases for v0.2-only constructs. This protects stored append-only streams and survives. The opening comment in `packages/toon/src/toonl.ts` is stale because it labels the complete implementation v0.1 only; update that comment to the unified v0.2 contract in the docs follow-up. |
| DOC-1 | Current-surface promises for TS aliases, Rust compatibility types, and `tq` no-op flags | **delete now / delete in the contract slice with the owning surface** | `docs/toon-official-spec.md`, `docs/migration-v4.md`, package/crate READMEs, `docs/npm-package.svg`, and the nested/keyed proposal stage tables mention surfaces approved above. Update each in the same follow-up as TS-1, RUST-2, or CLI-1 so documentation never leads or lags the code contract. Historical “before” examples may stay if explicitly labeled. |
| DOC-2 | v3.3 migration narrative, changelog entries, proposal benchmarks, and absorbed-extension design history | **genuinely load-bearing** | `docs/migration-v4.md` explains how stored v3.3 documents differ; `CHANGELOG.md` is an immutable release record; proposal tables retain measurement provenance; the nested/keyed specs explicitly label their text “design history” and Stage 4 “graduated.” These records do not select a runtime dialect. Retain them, tightening labels where a reader could confuse history with current usage. |
| DOC-3 | TypeScript TOONL module banner describing the implementation as v0.1 only | **delete now** | `packages/toon/src/toonl.ts` implements tagged lanes, continuation headers, and the other v0.2 additions, while its opening comment still says “TOONL v0.1.” This is a dialect-era current-surface claim with no consumer. [#331](https://github.com/reddb-io/toon/issues/331) replaces it with v0.2/unified-contract wording without removing the load-bearing v0.1 reader behavior in TOONL-1. |

## False positives closed by the sweep

- `parseRecords`, `parseStream`, `ToonlStream::parse`, and `Document::parse` are
  descriptive TOONL/model verbs, not aliases for the removed TypeScript root
  `parse` export.
- Query conditional `fallback` variables, parser fallback line numbers, release
  asset fallbacks, XML auto-detection, `.yml` as a YAML spelling, and syntax
  grammar fallback comments are current functional behavior, not backward-
  compatibility shims.
- Short/long CLI spellings such as `-s`/`--slurp`, plus `yaml`/`yml`, are
  documented current input vocabulary rather than deprecated names.
- `legacy_code` in the VS Code sample is user data chosen to exercise syntax
  highlighting.
- Rust `Record`/`ToonlReader` aliases and upstream-shaped TypeScript stream
  names have current role-based consumers; their alias syntax alone is not
  evidence of deprecation.
- Canonical extension fallback for ineligible values is a losslessness rule,
  not fallback to the legacy engine. Engine call sites that currently implement
  that rule remain owned by #315-#318 and #319.

## Removal ledger

| Approved removal | Follow-up | Timing |
| --- | --- | --- |
| TS package-root `parse`/`serialize` aliases and current-surface docs | [#328](https://github.com/reddb-io/toon/issues/328) | now |
| `tq` nested/keyed deprecated no-op flags and current-surface docs/tests | [#329](https://github.com/reddb-io/toon/issues/329) | now |
| Both packages' legacy public APIs, Rust compatibility option structs/adapters/model signatures, and dialect-suffixed canonical API | [#319](https://github.com/reddb-io/toon/issues/319), [#330](https://github.com/reddb-io/toon/issues/330) | one-engine contract slice |
| Stale TypeScript TOONL v0.1-only module banner | [#331](https://github.com/reddb-io/toon/issues/331) | now |

No source, test, fixture, generated artifact, or documentation removal is made
in audit #320. This slice adds only this report and the removal trackers.
