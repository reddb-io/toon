/**
 * Builds a JSON value tree from the decode event stream — the whole-document
 * convenience over `decodeFromLines` (ADR 0006). Mirrors upstream's
 * event-builder layering: the stream is the core, the tree is derived.
 */
import type { ToonEvent } from '../events.js';
import type { DecodeStreamOptions } from './stream.js';
export type JsonValue = string | number | boolean | null | JsonValue[] | {
    [key: string]: JsonValue;
};
export declare function buildValueFromEvents(events: Iterable<ToonEvent>): JsonValue;
export declare function decodeValue(input: string, options?: DecodeStreamOptions): JsonValue;
