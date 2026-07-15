/**
 * Type declarations for @reddb-io/toon.
 *
 * Hand-written on purpose: the package ships plain ESM with no build step, so
 * these are the contract, not a compiler artefact.
 */

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue }

/** Spaces per indentation level unless `ParseOptions.indent` says otherwise. */
export const DEFAULT_INDENT: 2

/** Decoder options (TOON spec §13). */
export interface ParseOptions {
  /** Spaces per indentation level. Default `2`. */
  indent?: number
  /** Enforce the §14 strict-mode error checklist. Default `true`. */
  strict?: boolean
  /** Expand dotted keys into nested objects (§13.4). Default off. */
  expandPaths?: 'safe' | boolean
}

/** A TOON decode failure, carrying the 1-based source line. */
export class ToonError extends Error {
  readonly line: number
  readonly reason: string
}

/** A TOONL decode or encode failure. `line` is `0` when there is no line context. */
export class ToonlError extends Error {
  readonly line: number
  readonly reason: string
}

/** Decodes a TOON document into a JSON value (spec §5 root-form discovery). */
export function parse(input: string, options?: ParseOptions): JsonValue

/** Decodes TOON whose root form is an object. Throws otherwise. */
export function parseDocument(
  input: string,
  options?: ParseOptions,
): { [key: string]: JsonValue }

/** Encodes a JSON value as canonical TOON (default profile). */
export function serialize(value: JsonValue): string

/** An object-rooted JSON value. */
export type JsonObject = { [key: string]: JsonValue }

/** Alias of `parse`, for consumers that speak encode/decode vocabulary. */
export const decode: typeof parse

/** Alias of `serialize`, for consumers that speak encode/decode vocabulary. */
export const encode: typeof serialize

/**
 * Encodes an object with a trailing spec-legal `summary:` field. Any existing
 * `summary` key is replaced and moved to the end; the result is one
 * conforming TOON document.
 */
export function appendSummaryField(value: JsonObject, summary: JsonValue): string

/**
 * Projects object rows onto an explicit minimal schema, preserving allowlist
 * order and dropping all non-allowlisted fields. Absent fields stay absent.
 */
export function projectFields<T extends Readonly<Record<string, JsonValue>>>(
  rows: readonly T[],
  fields: readonly string[],
): Array<Partial<T>>

/** One TOONL segment: a schema, and the raw (still-encoded) cells of its rows. */
export interface ToonlSegment {
  delimiter: string
  fields: string[]
  rows: string[][]
}

/** A flat TOONL record: field name to primitive. */
export type ToonlRecord = Record<string, null | boolean | number | string>

/** The delimiters a TOONL header may declare. */
export type ToonlDelimiter = ',' | '|' | '\t'

/** Parses a whole TOONL stream into its segments, keeping raw cells. */
export function parseStream(input: string): ToonlSegment[]

/** Every row of every segment of a TOONL stream, decoded into records. */
export function parseRecords(input: string): ToonlRecord[]

/** Closes a TOONL stream: one canonical TOON document per segment. */
export function closeTransform(input: string): string[]

/** Converts a whole JSON document string to canonical TOON. */
export function jsonToToon(input: string): string

/** Converts a whole TOON document string to compact JSON. */
export function toonToJson(input: string): string

/**
 * Decodes a TOONL stream record by record. Follows schema rotation, skips blank
 * lines, and validates every `[=N]` trailer against the rows seen.
 */
export function decodeLines(
  source: string | Iterable<string | Uint8Array> | AsyncIterable<string | Uint8Array>,
): AsyncGenerator<ToonlRecord, void, undefined>

export interface EncodeLinesOptions {
  /** Header delimiter. Default `','`. */
  delimiter?: ToonlDelimiter
  /** Write a `[=N]` trailer when a segment closes. Default `true`. */
  trailer?: boolean
}

/** An incremental TOONL emitter: lazy header, automatic rotation, optional trailer. */
export interface ToonlLineEmitter {
  /** Appends a record and returns the text to write. */
  push(record: ToonlRecord): string
  /** Closes the last segment and returns the text to write. */
  end(): string
}

export function encodeLines(options?: EncodeLinesOptions): ToonlLineEmitter

/** Encodes records to one TOONL string, rotating segments on schema change. */
export function encodeRecords(
  records: Iterable<ToonlRecord>,
  options?: EncodeLinesOptions,
): string

/** Web Streams decoder: TOONL `string | Uint8Array` chunks in, records out. */
export function ToonlDecodeStream(): TransformStream<string | Uint8Array, ToonlRecord>

/** Web Streams encoder: records in, TOONL text chunks out. */
export function ToonlEncodeStream(options?: EncodeLinesOptions): TransformStream<ToonlRecord, string>

/** JSONL text chunks in, TOONL text chunks out. */
export function JsonlToToonl(
  options?: EncodeLinesOptions,
): TransformStream<string | Uint8Array, string>

/** TOONL text chunks in, JSONL text chunks out. */
export function ToonlToJsonl(): TransformStream<string | Uint8Array, string>

/**
 * Maps or filters record streams and emits TOONL. Return `undefined` or `null`
 * to drop a record; schema rotation is handled by the output encoder.
 */
export function recordTransform(
  fn: (record: ToonlRecord) => ToonlRecord | null | undefined,
  options?: EncodeLinesOptions,
): TransformStream<ToonlRecord, string>

/** A single TOONL segment with a fixed schema, closed by its `[=N]` trailer. */
export class ToonlEncoder {
  constructor(delimiter: ToonlDelimiter, fields: readonly string[])
  readonly fields: string[]
  readonly rowCount: number
  /** Appends already-encoded cells. */
  pushRawRow(cells: readonly string[]): void
  /** Appends a record carrying exactly this segment's fields. */
  pushRow(record: ToonlRecord): void
  /** Closes the segment with its trailer and returns the whole text. */
  finish(): string
  /** The segment text so far, header included, without a trailer. */
  toString(): string
}
