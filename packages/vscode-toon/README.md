# RedDB Toon (`packages/vscode-toon`)

Syntax highlighting, strict-decode diagnostics, formatting, and JSON↔TOON
conversion for **TOON** (Token-Oriented Object Notation) and **TOONL** (the
line-oriented streaming layer) in VS Code.

**Naming.** TOON is the work of the
[toon-format](https://github.com/toon-format/spec) team. This extension ships
under the RedDB name — `reddb-io.reddb-toon`, display name **RedDB Toon** —
mirroring `@reddb-io/toon` (npm) and `reddb-io-toon` (crates.io), because it
covers the official v4.1 baseline, three userland wire extensions, and TOONL;
it deliberately does not claim the plain "toon" name.

## What it highlights

**TOON** (`.toon`, [pinned baseline](../../docs/toon-official-spec.md)):

- Array headers `key[N]{fields}:` with the length marker, the active-delimiter
  symbol (`[N|]`, tab), and the field list.
- Key-value lines, dotted keys, quoted keys, list items (`- `), quoted strings
  with the closed TOON v4.1 escape repertoire (unknown escapes flag as invalid),
  canonical numbers (leading-zero tokens like `05` stay string-colored, as they
  decode), `true`/`false`/`null`, and the empty array `[]`.
- Official nested field groups (`customer{name,country}`) and keyed tabular
  form (`people[2:]{first,last}:`).
- The three [reddb-io wire extensions](../../docs/toon-reddb-spec.md):
  primitive-array columns (`tags[;]`), object-array columns / fixed-width
  matrices (`values[3|]`), and cyclic discriminated arrays
  (`cycle(login,purchase,logout)*2`). Highlighting an extension does not make
  it official TOON syntax.

**TOONL** (`.toonl`, [spec](../../docs/toonl-reddb-spec.md)):

- Segment headers `[]{fields}:` (delimiter variants included), trailers `[=N]`,
  v0.2 continuation headers `[~]{...}:`, named schema declarations
  `[]<tag>{...}:`, and tagged rows `tag:cells`.
- Lines starting with the reserved `- ` prefix flag as invalid, matching the
  spec's MUST-reject rule.

A Markdown injection grammar also highlights ```` ```toon ```` and
```` ```toonl ```` fenced code blocks — the spec documents in `docs/` are full
of them.

TOON v4.1 full-line comments are highlighted when `#` is the first character
after zero or more spaces. TOONL has no comment syntax, so its language
configuration deliberately declares none.

## Editor features

The extension activates for `.toon`, `.toonl`, and JSON documents and runs the
same `@reddb-io/toon` codec the npm package ships (vendored into the `.vsix` by
`pnpm --filter reddb-toon build`).

- **Diagnostics.** Every `.toon` and `.toonl` document is strict-decoded as you
  type; the first error is underlined on its line, at its column when the
  decoder knows it, with the stable error `kind` as its code. Turn it off with
  `reddbToon.validate`.
- **Format Document.** Re-encodes a `.toon` document canonically, using the
  editor's tab size as the indent and `reddbToon.delimiter` for rows. A
  document with comment lines is left alone, because formatting goes through
  the JSON value and would drop them; so is a document that does not decode.
- **Commands.** *RedDB Toon: Convert JSON to TOON* and *Convert TOON to JSON*
  open the converted selection (or whole document) in a new editor; TOONL
  converts to a JSON array of records.
- **Status bar.** Shows the size and estimated tokens of a TOON document, and
  for JSON the token change TOON would bring (`TOON -38% tokens`), using the
  same estimator as `toon --stats`.

## Known limits

TextMate grammars are line-based and stateless, so the highlighter cannot track
the *active delimiter* per segment (all of `,`, `|`, and tab are treated as cell
separators everywhere) and cannot know which row tags were declared by a
`[]<tag>{...}:` schema. Both trade-offs only ever over-highlight; they never
hide structure.

## Install

One-liner from a GitHub release (the `.vsix` ships as a release asset from the
next stable release onward):

```sh
curl -fsSL https://github.com/reddb-io/toon/releases/latest/download/reddb-toon.vsix -o /tmp/reddb-toon.vsix && code --install-extension /tmp/reddb-toon.vsix
```

One-liner from a clone of this repository:

```sh
(cd packages/vscode-toon && pnpm build && pnpm dlx @vscode/vsce package -o reddb-toon.vsix) && code --install-extension packages/vscode-toon/reddb-toon.vsix
```

VSCodium and Cursor users: swap `code` for `codium` / `cursor`. Once the
extension is listed on the Marketplace and Open VSX (planned), the in-editor
one-liner becomes `Ctrl+P` → `ext install reddb-io.reddb-toon`.

Or run `pnpm build` here (it vendors the codec into `dist/codec/`), then press
`F5` with this folder open in VS Code to launch an Extension Development Host. `examples/sample.toon` and `examples/sample.toonl` exercise
every construct the grammars know about.

## Tests

```sh
pnpm test   # node --test — dependency-free grammar sanity + pattern behavior checks
```
