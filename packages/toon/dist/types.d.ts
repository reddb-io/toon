import type { DecodeStreamOptions } from './decode/stream.js';
import type { EncodeOptions } from './encode/serialize.js';
import type { EncodeReplacer } from './encode/replacer.js';
import type { ToonEvent } from './events.js';
export type JsonPrimitive = string | number | boolean | null;
export type JsonObject = {
    [key: string]: JsonValue | undefined;
};
export type JsonArray = JsonValue[] | readonly JsonValue[];
export type JsonValue = JsonPrimitive | JsonObject | JsonArray;
export type { Delimiter, DelimiterKey } from './constants.js';
export type { DecodeStreamOptions, EncodeOptions, EncodeReplacer };
export type DecodeOptions = DecodeStreamOptions;
export type JsonStreamEvent = ToonEvent;
export type ResolvedDecodeOptions = Readonly<{
    indentSize: number;
    strict: boolean;
}>;
export type ResolvedEncodeOptions = Readonly<{
    indentSize: number;
    delimiter: ',' | '|' | '\t';
    replacer?: EncodeReplacer;
}>;
