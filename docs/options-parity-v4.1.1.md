# Encode/decode options parity at upstream v4.1.1

This audit compares the released upstream JavaScript package and CLI with
`@reddb-io/toon`, the `reddb-io-toon` Rust crate, and their dedicated `toon`
binaries. `tq` remains a separate jq-compatible query interface and appears
below only where it consumes the same codec options. The immutable
implementation pin is `toon-format/toon` revision
`a9e6d97eca931379824f3b6a1ba8fbfbda7d3c53`; the accompanying specification pin
is `toon-format/spec` revision
`62f16b369408180f1faf1cba7da1b46d1f336f12`.

## Scope and method

The complete package inventory comes from the pinned upstream
[`EncodeOptions` and `DecodeOptions`](../vendor/toon/packages/toon/src/types.ts#L52-L101).
The complete CLI inventory comes from the pinned upstream
[`Options` table](../vendor/toon/packages/cli/README.md#options) and its
[`args` declaration](../vendor/toon/packages/cli/src/index.ts#L16-L58). There
are seven library entries and eight CLI entries. General help/version behavior
and the positional input are not encode/decode options.

Evidence is source-level where an API shape differs by language and executable
where the repositories share a behavior contract. In particular, the pinned
upstream resolver is in
[`packages/toon/src/index.ts`](../vendor/toon/packages/toon/src/index.ts#L201-L219),
the local TypeScript resolvers are in
[`encode/serialize.ts`](../packages/toon/src/encode/serialize.ts#L10-L58) and
[`decode/stream.ts`](../packages/toon/src/decode/stream.ts#L21-L36), the Rust
option types are in
[`encode.rs`](../crates/toon/src/lib_parts/encode.rs#L29-L53) and
[`stream.rs`](../crates/toon/src/lib_parts/stream.rs#L27-L48), and the two
`toon` front-ends map the upstream CLI in
[`run.ts`](../packages/toon/src/cli/run.ts) and
[`mod.rs`](../crates/toon/src/cli/mod.rs).

Classifications mean:

- **identical**: the local surfaces relevant to that option preserve upstream
  behavior over the upstream-documented input domain. An idiomatic Rust name
  or a local accepted-input superset does not change that behavior.
- **fixed-here**: a focused follow-up found and repaired the mismatch recorded
  by this audit.
- **ledgered divergence**: an actionable mismatch remains and is linked to a
  follow-up. The completed audit has no entries in this class.

## Library options: complete inventory

| Inventory ID | Pinned upstream behavior | `@reddb-io/toon` | Rust crate | `tq` | Classification and evidence |
| --- | --- | --- | --- | --- | --- |
| `encode.indentSize` | Optional number, default `2`; controls spaces per nesting level. The resolver passes the number through. | Same pass-through resolver, including zero, fractional, and rejected negative repeat counts. | `indent_size`, default `2`; preserves zero and every representable non-negative integer. | `--indent`, default `2`; applies upstream decimal `parseInt` normalization before passing the value to Rust. | **fixed-here** — [#308](https://github.com/reddb-io/toon/issues/308) adds zero, fractional/prefix, rejection, and positive-integer evidence across the three surfaces. |
| `encode.indent` | Deprecated alias for `indentSize`; default `2`, and `indentSize` wins when both are present. | Same deprecated alias, pass-through behavior, and precedence. | No deprecated JavaScript alias; idiomatic callers use `indent_size`. | `--indent` is the sole spelling. | **fixed-here** — alias precedence and zero pass-through are pinned by the package regression test added for [#308](https://github.com/reddb-io/toon/issues/308). |
| `encode.delimiter` | `','`, `'\t'`, or `'|'`; default comma; invalid values throw. It changes headers, rows, inline arrays, and quoting. | Same value set, default, validation, and wire effect. | `delimiter: char` accepts exactly the same three values and returns `EncodeError` otherwise. | `--delimiter` accepts the three literal values plus `comma`, `tab`, and `pipe` names. | **identical** — local accepted-input supersets produce the same bytes for every upstream value; package tests cover comma, tab, pipe, quoting, and invalid values. |
| `encode.replacer` | Optional `(key, value, path)` transform after normalization; `undefined` omits descendants, compacts arrays, and cannot omit the root. | Same callback order, path shape, normalization order, omission behavior, and root rule. | `encode_with_replacer` exposes the same behavior separately because a borrowed closure does not fit the copyable options struct. | Not applicable: the pinned upstream CLI does not expose a replacer either. | **identical** — local TypeScript tests mirror upstream replacer cases; Rust uses an idiomatic function boundary without changing semantics. |
| `decode.indentSize` | Optional number, default `2`; defines the expected spaces per level. | Same name/default and pass-through behavior; zero rejects non-empty input as invalid indentation. | `DecodeOptions.indent`, default `2`; zero likewise rejects non-empty input instead of being clamped. | `--indent`, default `2`, configures TOON decoding and output indentation. | **fixed-here** — [#308](https://github.com/reddb-io/toon/issues/308) aligns the Rust zero edge and exercises the TypeScript behavior. |
| `decode.indent` | Deprecated alias for `indentSize`; `indentSize` wins when both are present. | Same deprecated alias, behavior, and precedence. | `indent` is the canonical idiomatic field. | `--indent` is the sole spelling. | **fixed-here** — language-appropriate names preserve the same value and package alias precedence is covered explicitly. |
| `decode.strict` | Optional boolean, default `true`; `false` relaxes count, indentation, delimiter-consistency, malformed-header, and duplicate-key validation. | Same default and strict/non-strict outcomes, including last-write-wins duplicates. | Same default and event/tree decoder policy. | `--strict`/`--no-strict` maps directly to Rust. | **identical** — pinned upstream stream tests and the local shared event/conformance fixtures exercise both values. |

The Rust replacer is public as
[`encode_with_replacer`](../crates/toon/src/lib_parts/encode.rs#L78-L84).
Local TypeScript evidence is concentrated in
[`encoder.test.mjs`](../packages/toon/test/encoder.test.mjs) and strict/indent
evidence in [`toon.test.mjs`](../packages/toon/test/toon.test.mjs). Cross-language
strict behavior is also exercised by the fixtures under
[`tests/corpus/events`](../tests/corpus/events/README.md).

## Upstream CLI options: complete inventory

The local `toon` binary is the upstream-compatible JSON/TOON converter. It is
published by both `@reddb-io/toon` and `reddb-io-toon`; the TypeScript and Rust
front-ends share the same golden corpus, and the vendored upstream CLI suite is
replayed against both. `tq` deliberately keeps its jq-style query and
multi-format contract instead of overloading those flags.

| Inventory ID | Pinned upstream behavior | Local `toon` disposition | Classification and evidence |
| --- | --- | --- | --- |
| `cli.output` | `-o, --output <file>` writes conversion output to a path; omission means stdout. | Same path-writing and stdout behavior in both `toon` front-ends. | **fixed-here** — [#360](https://github.com/reddb-io/toon/issues/360) and [#361](https://github.com/reddb-io/toon/issues/361) add the dedicated TypeScript and Rust bins; [#362](https://github.com/reddb-io/toon/issues/362) replays the upstream CLI suite against both. |
| `cli.encode` | `-e, --encode` forces JSON-to-TOON mode instead of auto-detection. | Same explicit encode override and extension-based auto-detection. | **fixed-here** — the two bins added by [#360](https://github.com/reddb-io/toon/issues/360) and [#361](https://github.com/reddb-io/toon/issues/361) share this argument contract, with upstream-suite coverage from [#362](https://github.com/reddb-io/toon/issues/362). |
| `cli.decode` | `-d, --decode` forces TOON-to-JSON mode instead of auto-detection. | Same explicit decode override and extension-based auto-detection. | **fixed-here** — the two bins added by [#360](https://github.com/reddb-io/toon/issues/360) and [#361](https://github.com/reddb-io/toon/issues/361) share this argument contract, with upstream-suite coverage from [#362](https://github.com/reddb-io/toon/issues/362). |
| `cli.delimiter` | `--delimiter` accepts literal comma, tab, or pipe and affects TOON encode only. | Accepts all three literals plus readable names; affects TOON output only. | **identical** — every upstream invocation has the same wire result; the extra spellings are a compatible superset. |
| `cli.indent` | `--indent <number>`, default `2`, configures TOON encode/decode and decoded JSON indentation. Upstream uses `parseInt`, rejects negative/NaN, and admits zero. | Same decimal-prefix normalization, negative/NaN rejection, zero behavior, default, and three output roles. | **fixed-here** — [#308](https://github.com/reddb-io/toon/issues/308) pins zero, numeric-prefix, fractional, negative, and NaN-like cases. |
| `cli.stats` | `--stats` reports estimated JSON and TOON token counts and savings in encode mode. | `--stats` reports tokenx 1.3.0-compatible estimates to stderr for JSON-to-TOON stdin and file conversions without changing stdout. | **fixed-here** — [#309](https://github.com/reddb-io/toon/issues/309) adds the flag, versioned estimator, and public CLI regression coverage. |
| `cli.no-strict` | `--no-strict` disables strict decode validation; the positive `--strict` spelling restores it. | Supports both spellings and maps them directly to `DecodeOptions.strict`. | **identical** — defaults and observable recovery behavior match. |
| `cli.verbose` | `--verbose` adds stack traces and cause chains to conversion errors. | The TypeScript bin adds the cause chain and JavaScript stack; the Rust bin adds its cause chain because Rust has no equivalent JavaScript stack. | **fixed-here** — [#360](https://github.com/reddb-io/toon/issues/360) and [#361](https://github.com/reddb-io/toon/issues/361) implement the language-appropriate verbose boundary, and [#362](https://github.com/reddb-io/toon/issues/362) pins the upstream-facing behavior. |

The local CLI mapping is executable in
[`run.ts`](../packages/toon/src/cli/run.ts) and
[`mod.rs`](../crates/toon/src/cli/mod.rs). Their shared contract lives in
[`tests/golden/toon-cli`](../tests/golden/toon-cli/README.md), and the full
vendored compatibility ratchet is under
[`scripts/upstream-cli-suite`](../scripts/upstream-cli-suite/skip-ledger.json). The upstream
`--stats` calculation is visible in
[`conversion.ts`](../vendor/toon/packages/cli/src/conversion.ts#L29-L58), while
its strict UTF-8 and error presentation paths are separate CLI boundary
behavior rather than package decode options.

## Names investigated but absent at the pin

The issue brief named several historical or structural candidates to prevent
an assumption-driven inventory. None adds an option row at v4.1.1:

- **Length markers** are derived from the input and always emitted in array
  headers. Neither pinned options interface nor the CLI offers a toggle.
- **`keyFolding` and `flattenDepth`** are absent from upstream v4.1.1. Canonical
  encoders always preserve nesting; local canonical encoders do the same.
- **`expandPaths`** is absent from upstream v4.1.1. Canonical decoders preserve
  dotted keys literally, and that is now the only behavior: the local
  compatibility-only option went with the pre-v4 engine, as documented in the
  [v4.1 migration guide](migration-v4.md#1-path-expansion-removed-from-the-spec).

Conversely, local `maxDepth`, `reviver`, wire-extension flags, and TOONL flags
are not upstream v4.1.1 options and therefore are outside this parity
denominator. Their authority and status are recorded in the
[official baseline](toon-official-spec.md) and
[RedDB extension specification](toon-reddb-spec.md).

## Results and follow-ups

All 15 pinned entries are classified: 5 identical, 10 fixed-here, and 0 ledgered divergences.
No actionable gap remains open.

Issue [#308](https://github.com/reddb-io/toon/issues/308) closed the indent gap
with package, Rust, and `tq` regression tests. The
[#309](https://github.com/reddb-io/toon/issues/309) `cli.stats` fix includes
stdin/file coverage, a stable tokenx 1.3.0 estimator declaration, byte-for-byte
stdout assertions, and exact statistics diagnostics. Issues
[#360](https://github.com/reddb-io/toon/issues/360) and
[#361](https://github.com/reddb-io/toon/issues/361) added the TypeScript and
Rust `toon` binaries, and [#362](https://github.com/reddb-io/toon/issues/362)
made the pinned upstream CLI suite a two-front-end ratchet. The documentation
contract test prevents an option, classification total, or issue reference
from silently disappearing from the inventory.
