/**
 * TOONL v0.1 — the append-only, line-oriented streaming profile of TOON.
 * Semantics follow `docs/toonl-reddb-spec.md`: a stream is a sequence of segments,
 * each opened by a `[<delim?>]{fields}:` header, filled with one row per line,
 * and optionally closed by a `[=N]` trailer that asserts the row count.
 */
export declare function parseStream(input: any): {
    delimiter: any;
    fields: any;
    rows: any;
}[];
/** Every row of every segment, decoded into records. */
export declare function parseRecords(input: any): any[];
/**
 * Closes the stream: each segment becomes one canonical TOON document, so a
 * length-free append-only stream turns into length-bearing TOON (§ close
 * transform). Cells are already canonical, so they are re-emitted verbatim.
 */
export declare function closeTransform(input: any): string[];
export declare function closeTransformInterleaved(input: any): string[];
/**
 * Decodes a TOONL stream record by record, without ever holding the stream in
 * memory. Schema rotation is followed automatically, blank lines are skipped,
 * and each `[=N]` trailer is checked against the rows actually seen.
 *
 * `source` is a string, or an (async) iterable of string/Uint8Array chunks.
 */
export declare function decodeLines(source: any): AsyncGenerator<{}, void, unknown>;
export declare class ToonlReader {
    #private;
    constructor(source: any, options?: any);
    get cursor(): any;
    [Symbol.asyncIterator](): AsyncGenerator<{}, void, unknown>;
}
/**
 * Web Streams decoder: TOONL `string | Uint8Array` chunks in, records out.
 * It shares the same line grammar and trailer checks as `decodeLines`.
 */
export declare function ToonlDecodeStream(): import("stream/web").TransformStream<any, any>;
/** Web Streams encoder: records in, TOONL text chunks out. */
export declare function ToonlEncodeStream(options: any): import("stream/web").TransformStream<any, any>;
/** JSONL text chunks in, TOONL text chunks out. */
export declare function JsonlToToonl(options: any): import("stream/web").TransformStream<any, any>;
/** TOONL text chunks in, JSONL text chunks out. */
export declare function ToonlToJsonl(): import("stream/web").TransformStream<any, any>;
/**
 * Maps or filters record streams and emits TOONL. Return `undefined` or `null`
 * to drop a record; schema rotation is handled by the output encoder.
 */
export declare function recordTransform(fn: any, options: any): import("stream/web").TransformStream<any, any>;
/** Converts a whole JSON document string to canonical TOON. */
export declare function jsonToToon(input: any): string;
/** Converts a whole TOON document string to compact JSON. */
export declare function toonToJson(input: any): string;
/** A single TOONL segment: fixed schema, rows appended, closed by a trailer. */
export declare class ToonlEncoder {
    #private;
    constructor(delimiter: any, fields: any, options?: any);
    get fields(): any[];
    get rowCount(): number;
    setContinuationEveryRows(rows: any): void;
    setContinuationEveryBytes(bytes: any): void;
    /** Appends already-encoded cells, validating arity and each scalar. */
    pushRawRow(cells: any): void;
    /** Appends a record, which must carry exactly this segment's fields. */
    pushRow(record: any): void;
    /** Closes the segment with its `[=N]` trailer and returns the whole text. */
    finish(): string;
    /** The segment text so far, header included, without a trailer. */
    toString(): any;
}
/**
 * Incremental TOONL emitter. The header is written lazily with the first record,
 * a schema change rotates the segment automatically, and `end()` closes the last
 * one. Each call returns the text to append — nothing is buffered across calls.
 * Field order is canonicalized per record shape using the first order seen for
 * that shape, so later records with the same field set do not rotate solely
 * because their object keys arrived in a different order.
 *
 * `trailer` (default `true`) writes the `[=N]` trailer when a segment closes.
 */
export declare function encodeToonlLines({ delimiter, trailer, continuationEveryRows, continuationEveryBytes, }?: any): {
    push(record: any): string;
    declareLane(tag: any, declaredFields: any): string;
    pushTagged(tag: any, record: any): string;
    end(): string;
};
/**
 * Convenience: encodes records to one TOONL string, rotating on schema change.
 * Uses the same first-seen per-shape field order as `encodeToonlLines`.
 */
export declare function encodeRecords(records: any, options: any): string;
