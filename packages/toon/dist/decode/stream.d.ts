/**
 * Event-based streaming decoder (ADR 0006), targeting TOON spec v4.1.
 *
 * Consumes TOON lines (without newlines) and yields the six JSON-semantic
 * events, each carrying its 1-based source line. The parser requests lines on
 * demand and retains at most two classified content lines: the current line
 * plus one lookahead needed to distinguish a root scalar from a document.
 * Errors are fail-fast positioned `ToonError`s; strict-mode policy is resolved
 * here at the public boundary.
 *
 * Layering (§5–§12): line classification (comments, blanks, indentation) →
 * header grammar (§6) → scope emitters for objects (§8), arrays (§9.1–§9.4),
 * keyed tabular objects (§9.5) and objects as list items (§10).
 */
import type { ToonEvent } from '../events.js';
import { ToonError } from '../errors.js';
import { type ExtensionFieldNode } from './extension_events.js';
export interface DecodeStreamOptions {
    indent?: number;
    indentSize?: number;
    strict?: boolean;
    cyclicDiscriminatedArrays?: boolean;
    objectArrayColumns?: boolean;
    maxDepth?: number;
}
export interface FieldNode extends ExtensionFieldNode {
    children?: FieldNode[];
}
export { ToonError };
/**
 * Synchronously decodes lines with at most two classified lines of lookahead.
 */
export declare function decodeStreamSync(source: Iterable<string>, options?: DecodeStreamOptions): Generator<ToonEvent>;
/** Asynchronously decodes lines with the same bounded-lookahead parser. */
export declare function decodeStream(source: AsyncIterable<string> | Iterable<string>, options?: DecodeStreamOptions): AsyncGenerator<ToonEvent>;
/** Canonical line-to-event entry point, preserving the source's iteration mode. */
export declare function decodeFromLines(source: AsyncIterable<string>, options?: DecodeStreamOptions): AsyncGenerator<ToonEvent>;
export declare function decodeFromLines(source: Iterable<string>, options?: DecodeStreamOptions): Generator<ToonEvent>;
