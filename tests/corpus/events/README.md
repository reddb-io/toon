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

Consumed by `packages/toon/test/events.test.mjs` (TS) and the Rust runner
under `tests/runners/rust`. The fixtures are written before the decoders and
never edited to match an implementation — a mismatch is a decoder bug.

Event skeletons are generated from the vendored upstream reference
(`decodeStreamSync` at the v4.1.1 pin) as the oracle; the `line` field is our
ADR 0006 addition and is authored by hand from the input.
