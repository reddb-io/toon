<div align="center">

<img src="docs/hero.svg" alt="TOON &amp; TOONL — token-oriented data formats. tq CLI, Rust crate, JS/TS package." width="100%">

[![Release](https://img.shields.io/github/v/release/reddb-io/toon?include_prereleases&style=for-the-badge&color=ff2056&labelColor=0d1117)](https://github.com/reddb-io/toon/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/reddb-io/toon/ci.yml?branch=main&style=for-the-badge&label=CI&labelColor=0d1117)](https://github.com/reddb-io/toon/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge&labelColor=0d1117)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-linux%20%7C%20macOS%20%7C%20windows-8b949e?style=for-the-badge&labelColor=0d1117)](#prebuilt-binaries)

</div>

---

## Attribution

This is not the original TOON project. The TOON format was created by [Johann Schopplich](https://github.com/johannschopplich) — if you want to learn about or use TOON itself, follow the official repository at [github.com/toon-format/toon](https://github.com/toon-format/toon) and the official docs at [toonformat.dev](https://toonformat.dev).

This repository is the toolset that [RedDB](https://reddb.io) built on top of TOON for day-to-day use: a TypeScript library (`@reddb-io/toon`), a Rust crate, the `tq` CLI, the TOONL streaming extension, and opt-in RedDB extensions. Full credit goes to Johann and the toon-format team for the original format and spec.

---

## Formats

TOON is a token-oriented object notation for carrying structured JSON-shaped data through prompts and pipelines with less syntax overhead. It keeps the JSON data model, adds length-bearing tabular forms, and makes common truncation failures visible to decoders.

TOONL is the append-only stream form: one record per line, header once, and optional trailers for closed-stream verification. It is the streaming layer used by the JS package, Rust crate, and `tq`.

The root README is a hub, not the normative spec. Use these documents for detail:

- [Official TOON companion](docs/toon-official-spec.md): the upstream TOON behavior this repository tracks.
- [RedDB TOON extensions](docs/toon-reddb-spec.md): opt-in dialect features implemented here.
- [TOONL RedDB spec](docs/toonl-reddb-spec.md): append-only stream grammar and reader/writer behavior.
- [Dialect proposals](docs/proposals/): how this repository's dialect differs from the original TOON project and why each extension exists.

---

## What ships

<img src="docs/npm-package.svg" alt="@reddb-io/toon npm package banner" width="100%">

### `@reddb-io/toon` — JS/TS package

Dependency-free ESM for applications that need TOON in JavaScript, TypeScript, Node, Bun, Deno, or browsers. It parses TOON into plain JSON-shaped values, serializes those values back to canonical TOON, detects common truncation failures before a partial model response is trusted, and includes TOONL helpers for append-only record streams.

Use it when a prompt or pipeline wants compact structured data but the application still needs normal JSON objects at the edges.

```bash
pnpm add @reddb-io/toon
```

```js
import { parse, serialize } from '@reddb-io/toon'

const document = parse('users[2]{id,name}:\n  1,Ada\n  2,Linus\n')
console.log(document.users[0].name)
process.stdout.write(serialize(document))
```
```console
Ada
```
```console
users[2]{id,name}:
  1,Ada
  2,Linus
```

```js
import { encodeRecords, parseRecords } from '@reddb-io/toon'

const stream = encodeRecords([
  { id: 1, name: 'Ada' },
  { id: 2, name: 'Linus' },
])

process.stdout.write(stream)
console.log(JSON.stringify(parseRecords(stream)))
```
```console
[]{id,name}:
1,Ada
2,Linus
[=2]
```
```console
[{"id":1,"name":"Ada"},{"id":2,"name":"Linus"}]
```

Check completeness before accepting generated or streamed data:

```js
import { detectTruncation } from '@reddb-io/toon'

const report = detectTruncation('items[2]:\n  - one\n')
console.log(report.complete)
console.log(report.kind)
```
```console
false
```
```console
array_length_mismatch
```

Details: [`packages/toon`](packages/toon), [TOON spec companion](docs/toon-official-spec.md), [RedDB TOON extensions](docs/toon-reddb-spec.md), [TOONL spec](docs/toonl-reddb-spec.md), and the [truncation report model](docs/proposals/detect-truncation.md).

<img src="docs/rust-crate.svg" alt="reddb-io-toon Rust crate banner" width="100%">

### `reddb-io-toon` — Rust crate

The Rust library behind the CLI and a standalone crate for services that want TOON without shelling out. It provides the parser, serializer, ordered document model, lazy tabular arrays, truncation detector, JSON bridges, and TOONL reader/writer utilities used by `tq`.

Use it for Rust pipelines that need canonical TOON output, bounded parsing for untrusted input, or append-only TOONL streams that can be checked and resumed.

```toml
[dependencies]
reddb-io-toon = "0.8"
```

```rust
use reddb_io_toon::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = Value::parse_toon("users[2]{id,name}:\n  1,Ada\n  2,Linus\n")?;
    println!("{}", document.to_canonical_toon());
    Ok(())
}
```

Detect a truncated TOON document without losing the structured reason:

```rust
use reddb_io_toon::detect_truncation;

fn main() {
    let report = detect_truncation("items[2]:\n  - one\n");
    assert!(!report.complete);
    assert_eq!(report.to_json_value()["kind"], "array_length_mismatch");
}
```

Write and read a small TOONL stream:

```rust
use reddb_io_toon::{encode_toonl_values, ToonlReader, Value};
use std::io::Cursor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = vec![
        Value::from_json_str(r#"{"id":1,"name":"Ada"}"#)?,
        Value::from_json_str(r#"{"id":2,"name":"Linus"}"#)?,
    ];
    let stream = encode_toonl_values(&rows)?;
    let decoded = ToonlReader::new(Cursor::new(stream.as_bytes()))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(decoded, rows);
    Ok(())
}
```

Details: [`crates/toon`](crates/toon), [decoder and encoder options](docs/toon-official-spec.md), [RedDB extension rules](docs/toon-reddb-spec.md), [TOONL streaming format](docs/toonl-reddb-spec.md), and the [truncation report model](docs/proposals/detect-truncation.md).

<img src="docs/tq-cli.svg" alt="tq CLI banner" width="100%">

### `tq` — CLI

A jq-style command-line tool for inspecting data at the terminal. It queries TOON, JSON, YAML input, and TOONL rows with familiar field/index filters; converts between TOON, TOONL, and JSON; checks TOON or TOONL for truncation; and closes or trims append-only streams.

Use it as the fast path for shell pipelines: query a model response, turn JSON/YAML into compact TOON for a prompt, convert record streams to TOONL, or verify that a stream ended cleanly.

```bash
curl -fsSL https://raw.githubusercontent.com/reddb-io/toon/main/install.sh | sh
```

```bash
printf 'users[2]{id,name}:\n  1,Ada\n  2,Linus\n' \
  | tq '.users[].name'
```

Convert JSON records into TOONL for append-only logs:

```bash
printf '{"id":1,"name":"Ada"}\n{"id":2,"name":"Linus"}\n' \
  | tq -p json -o toonl .
```

Query YAML input and emit compact JSON:

```bash
printf 'users:\n  - id: 1\n    name: Ada\n' \
  | tq -p yaml -o json -c '.users[0]'
```

Check truncation before piping a partial document onward:

```bash
if ! printf 'items[2]:\n  - one\n' | tq check -p toon; then
  echo 'truncated input'
fi
```

Source install:

```bash
cargo install reddb-io-tq
```

Details: [`crates/tq`](crates/tq), [release assets](https://github.com/reddb-io/toon/releases), [TOON format detail](docs/toon-official-spec.md), [RedDB TOON extensions](docs/toon-reddb-spec.md), [TOONL streaming format](docs/toonl-reddb-spec.md), and [development commands](#develop).

<img src="docs/vscode-extension.svg" alt="RedDB Toon VS Code extension banner" width="100%">

### RedDB Toon — VS Code extension

Declarative syntax highlighting for `.toon` and `.toonl` files, plus `toon`/`toonl` fenced code blocks in Markdown. The TextMate grammars cover TOON v3.3 with all five RedDB wire extensions, and TOONL v0.1/v0.2 including trailers, continuation headers, named schemas, and tagged rows. Escape mistakes and the reserved TOONL `- ` prefix show up as errors while you type.

Use it when reading or writing TOON documents, TOONL streams, or the spec documents in [`docs/`](docs/) inside VS Code.

One-liner from a GitHub release (the `.vsix` ships as a release asset from the next stable release onward):

```bash
curl -fsSL https://github.com/reddb-io/toon/releases/latest/download/reddb-toon.vsix -o /tmp/reddb-toon.vsix && code --install-extension /tmp/reddb-toon.vsix
```

One-liner from a clone:

```bash
(cd packages/vscode-toon && pnpm dlx @vscode/vsce package -o reddb-toon.vsix) && code --install-extension packages/vscode-toon/reddb-toon.vsix
```

VSCodium and Cursor users: swap `code` for `codium` / `cursor`. Once the extension is listed on the Marketplace and Open VSX (planned), the in-editor one-liner becomes `Ctrl+P` → `ext install reddb-io.reddb-toon`.

Or open `packages/vscode-toon` in VS Code and press `F5` to try the grammars in an Extension Development Host against `examples/sample.toon` and `examples/sample.toonl`.

Details: [`packages/vscode-toon`](packages/vscode-toon), [TOON spec companion](docs/toon-official-spec.md), [RedDB TOON extensions](docs/toon-reddb-spec.md), and [TOONL streaming format](docs/toonl-reddb-spec.md).

---

## Navigation

| Need | Go to |
| --- | --- |
| Performance methodology and reports | [`benchmarks/`](benchmarks/) |
| TOON format detail | [`docs/toon-official-spec.md`](docs/toon-official-spec.md) and [`docs/toon-reddb-spec.md`](docs/toon-reddb-spec.md) |
| TOONL stream detail | [`docs/toonl-reddb-spec.md`](docs/toonl-reddb-spec.md) |
| RedDB dialect proposals | [`docs/proposals/`](docs/proposals/) |
| JavaScript and TypeScript package | [`packages/toon`](packages/toon) |
| Rust format crate | [`crates/toon`](crates/toon) |
| CLI crate and binary | [`crates/tq`](crates/tq) |
| VS Code extension | [`packages/vscode-toon`](packages/vscode-toon) |
| Releases and binary downloads | [GitHub releases](https://github.com/reddb-io/toon/releases) |

---

## Prebuilt binaries

Each release publishes `tq` binaries for Linux, macOS, and Windows, plus checksums and build provenance. The installer script resolves the matching asset for the current platform and installs or updates `tq` in place.

```bash
curl -fsSL https://raw.githubusercontent.com/reddb-io/toon/main/install.sh | sh
```

Useful installer knobs:

| Variable | Effect |
| --- | --- |
| `TQ_VERSION` | Pin a release tag |
| `TQ_CHANNEL` | Use `stable` or `next` |
| `TQ_INSTALL_DIR` | Choose the installation directory |
| `TQ_FORCE` | Reinstall even when already current |

---

## Develop

```bash
git clone https://github.com/reddb-io/toon
cd toon
git submodule update --init

cargo test --workspace
cargo run -p reddb-io-tq -- . deploys.toon

corepack enable
pnpm install
pnpm -r test
```

The Rust workspace contains `crates/toon` (`reddb-io-toon`) and `crates/tq` (`reddb-io-tq`). The pnpm workspace contains `packages/toon` (`@reddb-io/toon`). Release automation keeps all three on the same version.

## License

[MIT](LICENSE).
