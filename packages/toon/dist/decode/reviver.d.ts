import type { DecodeReviver, JsonValue } from '../types.js';
/** Apply an experimental decode reviver depth-first, from leaves to root. */
export declare function applyReviver(root: JsonValue, reviver: DecodeReviver): JsonValue;
