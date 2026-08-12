/**
 * @reddb-io/toon — TOON v4.1 encoder/decoder and TOONL v0.2 streaming, in
 * dependency-free ESM. The TOON side decodes to (and encodes from) plain JSON
 * values; the TOONL side is built for append-only streams.
 */
import { decodeValue } from './decode/build.js';
import { encode } from './encode/serialize.js';
export { VERSION } from './version.js';
export { ToonDecodeError, ToonError, ToonlCursorInvalidationError, ToonlError } from './errors.js';
export { DEFAULT_INDENT, DEFAULT_MAX_DEPTH } from './toon.js';
export { DEFAULT_DELIMITER, DELIMITERS } from './constants.js';
export { detectTruncation } from './decode/truncation.js';
export type { TruncationOptions, TruncationReport } from './decode/truncation.js';
export { encodeLines } from './encode/serialize.js';
export { rawString } from './encode/raw-string.js';
export type { RawString } from './encode/raw-string.js';
export { escapeString } from './lexical.js';
export { decodeStream, decodeStreamSync } from './decode/stream.js';
export type { FieldNode } from './decode/stream.js';
export { buildValueFromEvents, decodeFromLines } from './decode/build.js';
export type { ToonEvent, ToonEventType } from './events.js';
export type { DecodeOptions, DecodeReviver, DecodeStreamOptions, Delimiter, DelimiterKey, EncodeOptions, EncodeReplacer, JsonArray, JsonObject, JsonPrimitive, JsonStreamEvent, JsonValue, ResolvedDecodeOptions, ResolvedEncodeOptions, } from './types.js';
export { appendSummaryField, projectFields } from './helpers.js';
export declare const decode: typeof decodeValue;
export { encode };
export { JsonlToToonl, ToonlDecodeStream, ToonlEncodeStream, ToonlToJsonl, ToonlEncoder, ToonlReader, closeTransform, closeTransformInterleaved, decodeLines, encodeToonlLines, encodeRecords, jsonToToon, parseRecords, parseStream, recordTransform, toonToJson, } from './toonl.js';
