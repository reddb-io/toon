# Event-sequence fixtures — the cross-language parity contract

Each `*.json` file holds an array of cases. A case pins the exact decode event
sequence (ADR 0006) both implementations must emit for a document, or the
positioned error that must end the stream:

```json
{
  "name": "kebab-case-id",
  "input": "the TOON document as one string",
  "events": [ { "type": "key", "key": "a", "line": 1 } ],
  "error": { "line": 3 },
  "strict": true
}
```

- `events` — the full expected sequence, in order. Every event carries its
  1-based `line`. Runners compare `line` by default; a runner may be
  configured to ignore it, never the reverse.
- `error` — when present, `events` is the prefix emitted before the decoder
  fails with a positioned error on `line`. Error message text is
  implementation-owned and never pinned.
- `strict` — decoder mode for the case; defaults to `true`.

The corpus is split by form so a gap is easy to spot:

- `basics.json` — flat/nested objects, tabular arrays, comments, primitive lists.
- `scalars.json` — every scalar type and root primitive form (§2, §7).
- `arrays.json` — inline, list, and empty arrays, and the `,`/`|`/tab delimiters (§9.1–§9.4, §11).
- `tabular.json` — tabular arrays of objects, including nested field groups (§9.3).
- `keyed.json` — keyed tabular objects, `[N:]{…}` (§9.5).
- `nesting.json` — objects as list items, nested scopes, empty scopes (§8, §10).
- `lexical.json` — BOM, CRLF, trailing spaces, comments, blank lines, dotted/quoted keys (§5, §7, §12).
- `errors.json` — the §14 strict-mode error checklist, each pinned to its position.
- `nonstrict.json` — `strict:false` behaviours: LWW duplicate keys, tolerated counts, tab leniency (§12, §14).

Consumed by `packages/toon/test/events.test.mjs` (TS) and
`tests/runners/rust/toon/event_fixtures.rs` (Rust); CI runs both via
`pnpm -r test` and `cargo test --workspace`. The fixtures are the parity
contract between the two ports — a mismatch is a decoder bug, and they are
never edited to match an implementation.

Event skeletons are derived from the vendored upstream reference
(`decodeStreamSync` at the v4.1.1 pin) as the oracle — the emitted skeleton of
every clean-decode case is asserted equal to the oracle's. The `line` field is
our ADR 0006 addition. A handful of strict diagnostics (e.g. a blank line
inside a header span, §12/§14.2; a first line that is indented) are enforced by
our ports beyond what the v4.1.1 oracle flags; for those the TS↔Rust agreement
is the binding contract.
