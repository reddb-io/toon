/**
 * Event-based streaming decoder (ADR 0006), targeting TOON spec v4.1.
 *
 * Consumes an iterable of TOON lines (without newlines) and yields the six
 * JSON-semantic events, each carrying its 1-based source line. Errors are
 * fail-fast positioned `ToonError`s; strict-mode policy is resolved here at
 * the public boundary.
 *
 * Layering (§5–§12): line classification (comments, blanks, indentation) →
 * header grammar (§6) → scope emitters for objects (§8), arrays (§9.1–§9.4),
 * keyed tabular objects (§9.5) and objects as list items (§10).
 */
import type { ToonEvent } from '../events.js';
import { ToonError } from '../errors.js';
export interface DecodeStreamOptions {
    indent?: number;
    indentSize?: number;
    strict?: boolean;
    cyclicDiscriminatedArrays?: boolean;
    objectArrayColumns?: boolean;
    maxDepth?: number;
}
export interface FieldNode {
    name: string;
    children?: FieldNode[];
}
export declare function decodeStreamSync(source: Iterable<string>, options?: DecodeStreamOptions): Generator<ToonEvent>;
export { ToonError };
/**
 * Asynchronously decodes TOON lines into positioned events. Buffers the
 * source lines, then delegates to the sync core — incremental pull-based
 * classification is tracked for the cross-language fixture slice.
 */
export declare function decodeStream(source: AsyncIterable<string> | Iterable<string>, options?: DecodeStreamOptions): AsyncGenerator<ToonEvent>;
