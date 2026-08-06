/**
 * The decode event stream (ADR 0006).
 *
 * Six JSON-semantic events mirroring the upstream reference implementation's
 * `JsonStreamEvent`, each additionally carrying the 1-based source `line` it
 * was produced from. TOON forms (tabular, keyed, list) never appear in the
 * stream; errors never appear as events — any violation throws a positioned
 * `ToonDecodeError` and ends the stream.
 */
export type JsonPrimitive = string | number | boolean | null;
export type ToonEvent = {
    type: 'startObject';
    line: number;
} | {
    type: 'endObject';
    line: number;
} | {
    type: 'startArray';
    length: number;
    line: number;
} | {
    type: 'endArray';
    line: number;
} | {
    type: 'key';
    key: string;
    line: number;
} | {
    type: 'primitive';
    value: JsonPrimitive;
    line: number;
};
export type ToonEventType = ToonEvent['type'];
