# JSON-semantic positioned event stream with fail-fast errors and a drop-in API

The v4.1 decoders (ADR 0005) are rebuilt around an event stream. We adopted
upstream's six JSON-semantic events (`startObject`, `endObject`,
`startArray{length}`, `endArray`, `key{key}`, `primitive{value}`) — TOON forms
(tabular, keyed, list) stay invisible in the stream — but every event
additionally carries its source `line`, which upstream's events lack. The
position is what powers superior positioned diagnostics and stream-based
truncation detection; it is additive, so cross-language event fixtures can
compare it or ignore it by configuration.

Errors never appear as stream events: any violation is fail-fast — a thrown
`ToonDecodeError` in TypeScript, an `Err` in Rust — carrying line and error
code, with strict-mode policy resolved at the public decoder boundary and the
parsing helpers kept policy-free (upstream's proven layering). Truncated or
corrupt input recovery stays in the dedicated `detectTruncation` surface, not
in stream semantics.

The TypeScript package is API drop-in with upstream (`decode`, `decodeStream`,
`decodeStreamSync`, `encode`, `encodeLines`, matching signatures and defaults),
with our extensions as additional exports; Rust exposes the idiomatic
equivalent (`Decoder: Iterator<Item = Result<Event>>` plus a convenience
`decode()`). The shared event-sequence fixtures are the parity contract
between the two implementations and are written before the decoders.
