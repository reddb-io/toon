import type { DecodeStreamOptions } from './decode/stream.js'
import type { EncodeOptions } from './encode/serialize.js'
import type { EncodeReplacer } from './encode/replacer.js'
import type { ToonEvent } from './events.js'

export type JsonPrimitive = string | number | boolean | null
export type JsonObject = { [key: string]: JsonValue | undefined }
export type JsonArray = JsonValue[] | readonly JsonValue[]
export type JsonValue = JsonPrimitive | JsonObject | JsonArray

export type { Delimiter, DelimiterKey } from './constants.js'
export type { DecodeStreamOptions, EncodeOptions, EncodeReplacer }
/**
 * Experimental hook for transforming decoded values bottom-up.
 * Array indices are string keys while their path segments are numbers.
 * Returning `undefined` deletes a property or array element; at the root it
 * preserves the transformed root value.
 */
export type DecodeReviver = (
  key: string,
  value: JsonValue,
  path: readonly (string | number)[],
) => unknown

export interface DecodeOptions extends DecodeStreamOptions {
  /**
   * Experimental decode reviver audited from toon-format/toon#294 at
   * b3e4f61609ee7d676c0066440964a9f01ab767b7.
   * This option affects tree-building decode only, not event streams.
   */
  reviver?: DecodeReviver
}
export type JsonStreamEvent = ToonEvent

export type ResolvedDecodeOptions = Readonly<{
  indentSize: number
  strict: boolean
  reviver?: DecodeReviver
}>

export type ResolvedEncodeOptions = Readonly<{
  indentSize: number
  delimiter: ',' | '|' | '\t'
  replacer?: EncodeReplacer
}>
