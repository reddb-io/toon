# Encode/decode options parity at upstream v4.1.1

This audit compares the released upstream JavaScript package and CLI with
`@reddb-io/toon`, the `reddb-io-toon` Rust crate, and `tq`. The immutable
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
[`stream.rs`](../crates/toon/src/lib_parts/stream.rs#L27-L48), and `tq` maps
flags in [`args.rs`](../crates/tq/src/cli/args.rs#L58-L158).

Classifications mean:

- **identical**: the local surfaces relevant to that option preserve upstream
  behavior over the upstream-documented input domain. An idiomatic Rust name
  or a local accepted-input superset does not change that behavior.
- **fixed-here**: a focused follow-up found and repaired the mismatch recorded
  by this audit.
- **ledgered divergence**: the difference is either an intentional `tq`
  product/API choice recorded here or an actionable gap linked to a follow-up.

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

The upstream CLI is a dedicated JSON/TOON converter; `tq` is a jq-style query
and multi-format converter. Consequently, codec flags can match directly while
file routing and mode flags use `tq`'s established format-selection contract.

| Inventory ID | Pinned upstream behavior | `tq` disposition | Classification and evidence |
| --- | --- | --- | --- |
| `cli.output` | `-o, --output <file>` writes conversion output to a path; omission means stdout. | `-o` selects an output format, and stdout/redirection owns the destination path. Reusing it for a path would break jq-compatible scripts. | **ledgered divergence** — deliberate CLI product contract; codec output remains redirectable. |
| `cli.encode` | `-e, --encode` forces JSON-to-TOON mode instead of auto-detection. | `-p json -o toon` is the explicit equivalent; `-e` already implements jq exit-status semantics. | **ledgered divergence** — deliberate flag vocabulary and a real short-option collision. |
| `cli.decode` | `-d, --decode` forces TOON-to-JSON mode instead of auto-detection. | `-p toon -o json` is the explicit equivalent. | **ledgered divergence** — deliberate multi-format selector rather than a binary mode switch. |
| `cli.delimiter` | `--delimiter` accepts literal comma, tab, or pipe and affects TOON encode only. | Accepts all three literals plus readable names; affects TOON output only. | **identical** — every upstream invocation has the same wire result; the extra spellings are a compatible superset. |
| `cli.indent` | `--indent <number>`, default `2`, configures TOON encode/decode and decoded JSON indentation. Upstream uses `parseInt`, rejects negative/NaN, and admits zero. | Same decimal-prefix normalization, negative/NaN rejection, zero behavior, default, and three output roles. | **fixed-here** — [#308](https://github.com/reddb-io/toon/issues/308) pins zero, numeric-prefix, fractional, negative, and NaN-like cases. |
| `cli.stats` | `--stats` reports estimated JSON and TOON token counts and savings in encode mode. | `--stats` reports tokenx 1.3.0-compatible estimates to stderr for JSON-to-TOON stdin and file conversions without changing stdout. | **fixed-here** — [#309](https://github.com/reddb-io/toon/issues/309) adds the flag, versioned estimator, and public CLI regression coverage. |
| `cli.no-strict` | `--no-strict` disables strict decode validation; the positive `--strict` spelling restores it. | Supports both spellings and maps them directly to `DecodeOptions.strict`. | **identical** — defaults and observable recovery behavior match. |
| `cli.verbose` | `--verbose` adds stack traces and cause chains to conversion errors. | Errors are intentionally a bounded `error: …` diagnostic; there is no stack-trace mode. | **ledgered divergence** — diagnostic presentation is outside codec semantics and follows `tq`'s stable jq-style boundary. |

The local CLI mapping is executable in
[`cli/mod.rs`](../crates/tq/src/cli/mod.rs#L70-L105) and
[`cli/output.rs`](../crates/tq/src/cli/output.rs#L45-L73). The upstream
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

All 15 pinned entries are classified: 5 identical, 6 fixed-here, and 4
ledgered divergences. All four ledgered rows are deliberate CLI adaptations or
diagnostic presentation differences; no actionable gap remains open.

Issue [#308](https://github.com/reddb-io/toon/issues/308) closed the indent gap
with package, Rust, and `tq` regression tests. The
[#309](https://github.com/reddb-io/toon/issues/309) `cli.stats` fix includes
stdin/file coverage, a stable tokenx 1.3.0 estimator declaration, byte-for-byte
stdout assertions, and exact statistics diagnostics. The documentation contract
test prevents an option or issue reference from silently disappearing from the
inventory.
