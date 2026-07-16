# RedDB Toon (`packages/vscode-toon`)

Syntax highlighting for **TOON** (Token-Oriented Object Notation) and **TOONL**
(the line-oriented streaming layer) in VS Code, as a declarative TextMate
extension — no activation code.

**Naming.** TOON is the work of the
[toon-format](https://github.com/toon-format/spec) team. This extension ships
under the RedDB name — `reddb-io.reddb-toon`, display name **RedDB Toon** —
mirroring `@reddb-io/toon` (npm) and `reddb-io-toon` (crates.io), because it
covers this repository's *flavor* of the format (the five wire extensions and
TOONL) and deliberately does not claim the plain "toon" name.

## What it highlights

**TOON** (`.toon`, [spec companion](../../docs/toon-official-spec.md)):

- Array headers `key[N]{fields}:` with the length marker, the active-delimiter
  symbol (`[N|]`, tab), and the field list.
- Key-value lines, dotted keys, quoted keys, list items (`- `), quoted strings
  with the closed v3.3 escape repertoire (unknown escapes flag as invalid),
  canonical numbers (leading-zero tokens like `05` stay string-colored, as they
  decode), `true`/`false`/`null`, and the empty array `[]`.
- All five [reddb-io wire extensions](../../docs/toon-reddb-spec.md): nested
  tabular headers (`customer{name,country}`), keyed-map collapse
  (`people{first,last}:`), primitive-array columns (`tags[;]`), object-array
  columns / fixed-width matrices (`values[3|]`), and cyclic discriminated
  arrays (`cycle(login,purchase,logout)*2`).

**TOONL** (`.toonl`, [spec](../../docs/toonl-reddb-spec.md)):

- Segment headers `[]{fields}:` (delimiter variants included), trailers `[=N]`,
  v0.2 continuation headers `[~]{...}:`, named schema declarations
  `[]<tag>{...}:`, and tagged rows `tag:cells`.
- Lines starting with the reserved `- ` prefix flag as invalid, matching the
  spec's MUST-reject rule.

A Markdown injection grammar also highlights ```` ```toon ```` and
```` ```toonl ```` fenced code blocks — the spec documents in `docs/` are full
of them.

Neither format has comment syntax, so the language configurations deliberately
declare none (`#` is data).

## Known limits

TextMate grammars are line-based and stateless, so the highlighter cannot track
the *active delimiter* per segment (all of `,`, `|`, and tab are treated as cell
separators everywhere) and cannot know which row tags were declared by a
`[]<tag>{...}:` schema. Both trade-offs only ever over-highlight; they never
hide structure.

## Install locally

```sh
pnpm dlx @vscode/vsce package   # from packages/vscode-toon → reddb-toon-<version>.vsix
code --install-extension reddb-toon-*.vsix
```

Or press `F5` with this folder open in VS Code to launch an Extension
Development Host. `examples/sample.toon` and `examples/sample.toonl` exercise
every construct the grammars know about.

## Tests

```sh
pnpm test   # node --test — dependency-free grammar sanity + pattern behavior checks
```
