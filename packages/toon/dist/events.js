/**
 * The decode event stream (ADR 0006).
 *
 * Six JSON-semantic events mirroring the upstream reference implementation's
 * `JsonStreamEvent`, each additionally carrying the 1-based source `line` it
 * was produced from. TOON forms (tabular, keyed, list) never appear in the
 * stream; errors never appear as events — any violation throws a positioned
 * `ToonDecodeError` and ends the stream.
 */
export {};
