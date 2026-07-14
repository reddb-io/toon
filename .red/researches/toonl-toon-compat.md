# Research: TOONL vs TOON v3.3 compatibility + upstream streaming state

Resolves [#33](https://github.com/reddb-io/tq/issues/33). Feeds the decision: ship TOONL as our own
extension vs "compatible by construction" with TOON v3.3.

- Spec under test: `vendor/toon-spec/SPEC.md` @ `f55b93a` (TOON v3.3) — §5 root form, §6 header
  grammar (ABNF), §9.3 tabular form, §12 whitespace, §14 strict-mode checklist.
- Decoder under test: `crates/toon` (`reddb_io_toon::Value::parse_with_options`), current `main`
  (`adc5620`), probed in both `strict=true` (default) and `strict=false`.
- Probe harness: appendix A (a `cargo run -p reddb-io-toon --example toonl_probe` throwaway).

## TL;DR

1. **No TOONL candidate syntax is valid TOON v3.3 while the stream is open.** The `[N]` count is
   normative in the header ABNF (§6: `bracket-seg = "[" length [ delimsym ] "]"`, `length` is a
   mandatory non-negative integer). A countless header, a repeated root header, and a `[=N]`
   trailer are each rejected by the grammar — confirmed empirically against our decoder.
2. **A *completed* TOONL stream is one cheap O(n) transform away from a valid TOON document** —
   *if* TOONL keeps rows at TOON row depth (2-space indent). The transform is: rewrite the header
   line to `[N]{fields}:` and drop the trailer. If TOONL uses flush-left rows (the JSONL-ish
   choice), every row must also be re-indented, so the transform is still O(n) single-pass but
   touches every line. In-place header patching is impossible either way: `[03]`-style padding is
   forbidden (§6, no leading zeros) and content between `]` and `{`/`:` is forbidden, so the count
   rewrite always shifts bytes.
3. **Two silent-misparse footguns** (wrong value, no error, even in strict mode): a lone
   `{id,name}:` line decodes as `{"{id,name}": {}}`, and a single bare row `1,alice` decodes as
   the primitive string `"1,alice"`. Every other candidate fails loudly. TOONL docs fed to TOON
   decoders mostly error out cleanly — they do not corrupt data, with those two exceptions.
4. **Upstream has explicitly and repeatedly declined format-level streaming** (spec#15 `[-]`
   declined, toon#120 "TOON Lines" declined, toon#163 + PRs rejected). `[N]` is called "a central
   invariant of TOON". Streaming lives only in library APIs (`encodeLines`, `decodeStream`).
   Nothing on the v4 roadmap. No third-party lines-format proposal exists. The niche is open, and
   the syntax TOONL needs (`{f}:` / `[]{f}:` headers, `[=N]` trailer) is invalid TOON today and —
   per upstream's own VERSIONING.md — could only be claimed by upstream in a breaking MAJOR.

**Recommendation:** ship TOONL as our own extension (own media hint, e.g. `.toonl`), designed for
*cheap convertibility* rather than by-construction compatibility — which is unattainable for an
append-only file under TOON v3.3. The one design lever that matters for conversion cost: whether
rows carry the 2-space TOON indent (header-only close) or are flush-left (full-file re-indent on
close, but simpler/JSONL-like appends). Both remain O(n) stream transforms; neither allows an
in-place close.

## 1. Empirical compat matrix

Input fed as a whole document to `reddb_io_toon::Value::parse_with_options`. "OK" cells show the
decoded JSON. Control row first.

| # | Candidate | Input sketch | strict=true | strict=false |
|---|-----------|--------------|-------------|--------------|
| C1 | **Control**: counted tabular, rows indented | `[2]{id,name}:` + 2 indented rows | OK `[{...},{...}]` | OK |
| C2 | Counted tabular, rows **flush-left** | `[2]{id,name}:` + 2 flush rows | ERROR L2 `array length mismatch` (flush lines are not rows) | same |
| A1 | Countless brace header, rows indented | `{id,name}:` + indented rows | ERROR L2 `expected `key: value`` | same |
| A2 | Countless brace header, rows flush-left | `{id,name}:` + flush rows | ERROR L2 `expected `key: value`` | same |
| B1 | Empty-bracket header, rows indented | `[]{id,name}:` + indented rows | ERROR L1 `invalid array header` | ERROR L2 `expected `key: value`` |
| B2 | Empty-bracket header, rows flush-left | `[]{id,name}:` + flush rows | ERROR L1 `invalid array header` | ERROR L2 |
| D1 | Second root header mid-document (rotation, new schema) | `[2]{id,name}:` rows `[2]{id,city}:` rows | ERROR L4 `expected end of document` | same |
| D2 | Second root header, identical schema | same shape | ERROR L4 `expected end of document` | same |
| E1 | Trailer `[=2]` flush-left after counted rows | `[2]{...}:` rows `[=2]` | ERROR L4 `expected end of document` | same |
| E2 | Trailer `[=2]` at row depth | `[2]{...}:` rows `␠␠[=2]` | ERROR L4 `array length mismatch` (trailer eaten as a bad row) | same |
| F1 | Bare rows, no header | `1,alice` / `2,bob` | ERROR L1 `expected `key: value`` | same |
| F2 | **Single** bare row | `1,alice` | **OK — silent misparse** as primitive string `"1,alice"` (§5 single-line rule) | same |
| G1 | Blank line between counted rows | `[2]{...}:` row, blank, row | ERROR L4 `blank line inside array` | **OK** (blank ignored, spec-conform §12) |
| H1 | Full TOONL: `{id,name}:` + flush rows + `[=2]` | | ERROR L2 | same |
| H2 | Full TOONL: `[]{id,name}:` + indented rows + `[=2]` | | ERROR L1 | ERROR L2 |
| I1 | **Closed-rewrite**: header count patched, trailer dropped | `[2]{id,name}:` + indented rows | OK | OK |
| J1 | Keyed countless header | `rows{id,name}:` + indented rows | ERROR L2 | same |
| J2 | Keyed empty-bracket header | `rows[]{id,name}:` + indented rows | ERROR L1 | ERROR L2 |
| K1 | Stale count: `[3]` with 2 rows (truncated read) | | ERROR L3 `array length mismatch` | same |
| K2 | Stale count: `[1]` with 2 rows (grown file) | | ERROR L3 `array length mismatch` | same |
| L1 | Lone `{id,name}:` line | | **OK — silent misparse** `{"{id,name}":{}}` | same |
| L2 | Lone `[]{id,name}:` line | | ERROR L1 | **OK — misparse** `{"[]{id,name}":{}}` |
| M1 | Lone `[=2]` line | | ERROR L1 `array header missing colon` | **OK — misparse** `"[=2]"` |
| N1 | Upstream-declined `[-]{id,name}:` (spec#15 syntax) | + indented rows | ERROR L1 `invalid array header` | ERROR L2 |

### Readings

- **What breaks:** everything TOONL needs while open — countless headers (A, B, J), header
  rotation (D), trailers (E, M), bare rows (F1) — plus any stale-count read of a growing file
  (K1/K2, in *both* modes: our decoder enforces count/width even with `strict=false`, which is
  stricter than the spec's §14.1 strict-only framing).
- **What misparses silently** (the real hazard): L1 `{id,name}:` → object with literal key
  `{id,name}` (both modes — per §6 non-strict fallback the line is a key-value line, and our
  strict path only rejects when a *bracket* segment is present); F2 single bare row → root
  primitive string (§5 single-line rule); L2/M1 in non-strict mode. A TOONL file with a header
  and zero rows, or one data row and no header, is *valid TOON meaning something else*.
- **What's compatible:** blank lines between rows are already tolerated by non-strict TOON (G1),
  and a closed-and-rewritten TOONL segment is canonical TOON (I1 = C1).

## 2. Is a completed TOONL stream expressible as valid TOON v3.3?

**As-is: no, under every candidate syntax** (rows H1/H2). Three independent grammar conflicts:

1. §6 ABNF makes `length` mandatory inside `bracket-seg`; `{f}:`, `[]{f}:` and `[-]{f}:` are all
   non-headers. Strict decoders must error; non-strict decoders fall through to *key-value*
   parsing (the misparse hazard above).
2. §5 root form admits exactly one root array; a second depth-0 header (rotation) makes the
   document invalid (`expected end of document`). Multi-segment TOONL can never be one TOON doc —
   at best each segment maps to one doc (framing, which upstream explicitly ruled out of scope,
   see toon#120).
3. A trailer line after the rows is unreachable grammar at depth 0 (E1) and a malformed row at
   depth 1 (E2). `[=N]` cannot be expressed anywhere in a TOON document.

**Cheapest transformation to valid TOON on close** (single segment):

- *If TOONL rows are at TOON row depth (2-space indent):* rewrite line 1 from `{fields}:` to
  `[N]{fields}:` and drop the trailer. Two-line touch; O(n) only because the header rewrite
  shifts every subsequent byte in a flat file. As a *stream* transform (TOONL→TOON pipe) it is
  header-only.
- *If TOONL rows are flush-left:* additionally prepend 2 spaces to every row (C2 proves counted
  headers do not rescue flush rows). Still a trivial single-pass transform, but it touches every
  line, and byte-offset indexes into the TOONL file don't survive.
- *In-place close (no byte shifting) is impossible in valid TOON:* the spec forbids
  zero-padded lengths (`[03]`, §6), forbids content between `]` and `{`/`:` (§6), and forbids
  trailing spaces (§12), so there is no legal place to pre-reserve count width. A closed TOONL
  file therefore either keeps its trailer (and stays TOONL), or is rewritten/streamed into TOON.
- N for the rewrite comes free from the `[=N]` trailer (its purpose), or from counting rows.

## 3. Upstream state (toon-format org, July 2026)

Latest spec is **v3.3** (Working Draft 2026-05-21). Full trail with sources:

| Thread | State | Outcome |
|---|---|---|
| [spec#15](https://github.com/toon-format/spec/issues/15) — RFC: unknown-size arrays, `[-]` length | closed | **Declined.** Maintainer: the concrete `[N]` is "a central invariant of TOON"; streaming decode in the TS impl is the supported path. Unknown-length *emission* stays impossible. |
| [toon#120](https://github.com/toon-format/toon/issues/120) — JSON Lines / "TOON Lines" (`---` framing) | closed | **Declined.** "JSON Lines is a framing format… TOON is deliberately one JSON value per document." JSONL support "will live at the tooling level." |
| [toon#131](https://github.com/toon-format/toon/issues/131) — Streaming support (incl. `key[]{fields}` suggestion) | closed | Resolved as **library feature only**: `encodeLines()`, `decodeStream[Sync]()` in the TS core ([API docs](https://toonformat.dev/reference/api)). No format change; `[N]` stays mandatory. |
| [toon#163](https://github.com/toon-format/toon/issues/163) + PRs [#167](https://github.com/toon-format/toon/pull/167), [#176](https://github.com/toon-format/toon/pull/176) | closed | Multi-doc framing / batch APIs **rejected** as out of core scope. |
| [Discussion #166](https://github.com/toon-format/toon/discussions/166) | — | Maintainer position: "The spec will stay focused on the data model + concrete syntax. Streaming… are library/tooling concerns." |
| [spec#34](https://github.com/toon-format/spec/issues/34) — streaming-compatible compliance profile | open | Unresolved; maintainer skeptical of fragmentation. About *parser* profiles (path expansion/key folding), not a lines format. |
| [spec#48](https://github.com/toon-format/spec/issues/48) / [spec#49](https://github.com/toon-format/spec/issues/49) — v4 roadmap | open | v4 is tabular generalization only. **No streaming/lines/append item on any roadmap.** |
| [toon-rust#65](https://github.com/toon-format/toon-rust/pull/65) | merged | Streaming *encode* feature (serde) — library-level again. |

**Versioning/extensibility policy** ([VERSIONING.md](https://github.com/toon-format/spec/blob/main/VERSIONING.md),
SPEC.md §18): `MAJOR.MINOR` SemVer; "adding new reserved characters that could conflict with
existing valid TOON documents" is explicitly a breaking (MAJOR) change; structural characters
(colon, brackets, braces, hyphen) keep their meanings across versions. **No third-party extension
mechanism, no reserved extension syntax, nothing about variants.** Consequence for us: upstream
cannot adopt countless headers or `[=N]` in any v3.x, and our use of currently-*invalid* TOON
syntax cannot collide with a future MINOR — only with a hypothetical v4+, which today contains no
streaming plans.

**Third-party prior art:** [jackson-toon](https://github.com/prb/jackson-toon) (streaming *parser*,
motivated spec#34), [pytoon-codec](https://github.com/DiogoRibeiro7/pytoon-codec) (logs-oriented
codec, not a format), a throwaway `toonl` shell wrapper in toon#120. **No published "TOON Lines" /
NDTOON format proposal exists** on GitHub, HN, or Reddit — the name and the niche are unclaimed.

## 4. Decision input

- "Compatible by construction" is **not cheap — it is impossible** for the open/append phase: TOON
  v3.3 structurally requires the element count before the first row, and upstream has declined
  every mechanism (spec#15, toon#120) that would lift that.
- What *is* cheap is **convertible by construction**: pick TOONL syntax that (a) is invalid TOON
  (loud errors, no silent misparse — prefer `[]{fields}:` over bare `{fields}:`, since the latter
  silently decodes as an object key, row L1) and (b) becomes canonical TOON via a header-only
  stream transform (keep rows at 2-space indent) or a trivial line transform (flush-left rows).
- Mid-stream header rotation permanently forfeits single-document TOON equivalence; a rotated
  TOONL file maps to a *sequence* of TOON docs — exactly the framing upstream refuses to define,
  which is the strongest argument that TOONL is our extension, not a TOON profile.

## Appendix A: probe harness

Run from the repo root inside the research worktree (file:
`crates/toon/examples/toonl_probe.rs`, not committed):

```rust
use reddb_io_toon::{ParseOptions, Value};

fn probe(name: &str, input: &str) {
    println!("=== {name} ===");
    for strict in [true, false] {
        let options = ParseOptions { strict, ..ParseOptions::default() };
        match Value::parse_with_options(input, options) {
            Ok(value) => println!("strict={strict} => OK {}", value.to_json_string(true).unwrap()),
            Err(e) => println!("strict={strict} => ERROR line {}: {}", e.line(), e.message()),
        }
    }
}

fn main() {
    probe("A1", "{id,name}:\n  1,alice\n  2,bob");
    probe("A2", "{id,name}:\n1,alice\n2,bob");
    probe("B1", "[]{id,name}:\n  1,alice\n  2,bob");
    probe("B2", "[]{id,name}:\n1,alice\n2,bob");
    probe("C1", "[2]{id,name}:\n  1,alice\n  2,bob");
    probe("C2", "[2]{id,name}:\n1,alice\n2,bob");
    probe("D1", "[2]{id,name}:\n  1,alice\n  2,bob\n[2]{id,city}:\n  3,paris\n  4,tokyo");
    probe("D2", "[2]{id,name}:\n  1,alice\n  2,bob\n[2]{id,name}:\n  3,carol\n  4,dave");
    probe("E1", "[2]{id,name}:\n  1,alice\n  2,bob\n[=2]");
    probe("E2", "[2]{id,name}:\n  1,alice\n  2,bob\n  [=2]");
    probe("F1", "1,alice\n2,bob");
    probe("F2", "1,alice");
    probe("G1", "[2]{id,name}:\n  1,alice\n\n  2,bob");
    probe("H1", "{id,name}:\n1,alice\n2,bob\n[=2]");
    probe("H2", "[]{id,name}:\n  1,alice\n  2,bob\n[=2]");
    probe("I1", "[2]{id,name}:\n  1,alice\n  2,bob");
    probe("J1", "rows{id,name}:\n  1,alice\n  2,bob");
    probe("J2", "rows[]{id,name}:\n  1,alice\n  2,bob");
    probe("K1", "[3]{id,name}:\n  1,alice\n  2,bob");
    probe("K2", "[1]{id,name}:\n  1,alice\n  2,bob");
    probe("L1", "{id,name}:");
    probe("L2", "[]{id,name}:");
    probe("M1", "[=2]");
    probe("N1", "[-]{id,name}:\n  1,alice\n  2,bob");
}
```
