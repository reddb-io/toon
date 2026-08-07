/**
 * Builds a JSON value tree from the decode event stream — the whole-document
 * convenience over `decodeFromLines` (ADR 0006). Mirrors upstream's
 * event-builder layering: the stream is the core, the tree is derived.
 */
import type { ToonEvent } from '../events.js';
import type { DecodeOptions, JsonValue } from '../types.js';
export declare function buildValueFromEvents(events: Iterable<ToonEvent>): JsonValue;
/** Decodes pre-split TOON lines into one JSON value. */
export declare function decodeFromLines(lines: Iterable<string>, options?: DecodeOptions): JsonValue;
export declare function decodeValue(input: string, options?: DecodeOptions): JsonValue;
