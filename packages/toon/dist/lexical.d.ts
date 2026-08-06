/**
 * Scalars, quoted strings, keys, delimiters and numbers — the lexical layer
 * TOON (§4, §7, §11) and TOONL both build on.
 */
/** The document delimiter of the default profile (spec §11.1). */
export declare const DOCUMENT_DELIMITER = ",";
/**
 * Splits like Rust's `str::lines`: on `\n`, dropping the trailing empty piece a
 * final newline would otherwise produce, and stripping a `\r` before each `\n`.
 */
export declare function splitLines(input: any): any;
/** Decodes a scalar token (spec §4): quoted string, literal, number, or bare string. */
export declare function parseScalar(value: any, line: any): any;
/** Returns `[key, quoted]`. An empty key is only legal when it was quoted. */
export declare function parseKey(value: any, line: any): any[];
export declare function parseQuotedString(value: any, line: any): string;
/** Splits on unquoted occurrences of `delimiter`, preserving empty tokens (§11.2). */
export declare function splitDelimited(value: any, delimiter: any, line: any): any[];
/** Index of the first `needle` outside a quoted string, or `-1`. */
export declare function findUnquoted(value: any, needle: any, line: any): any;
/**
 * A decoder-visible number: `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`.
 * Leading zeros in the integer part make the token a string (§4).
 */
export declare function isNumberToken(value: any): boolean;
/**
 * The §7.2 "numeric-like" test used for quoting: unlike {@link isNumberToken} it
 * also matches leading-zero forms such as `05`, which decode as strings but must
 * still be quoted so they never decode as numbers.
 */
export declare function isNumericLike(value: any): boolean;
/** Canonical decimal form per §2. JS already prints the shortest round-trip form. */
export declare function numberText(value: any): string;
export declare function isPrimitive(value: any): boolean;
export declare function primitiveText(value: any, delimiter: any): any;
/** Unquoted keys must match `^[A-Za-z_][A-Za-z0-9_.]*$` (§7.3). */
export declare function isBareKey(value: any): boolean;
export declare function canonicalKey(value: any): any;
export declare function canonicalString(value: any, delimiter: any): any;
/** The §7.2 quoting checklist. */
export declare function needsQuotes(value: any, delimiter: any): any;
export declare function quoteString(value: any): string;
/**
 * Defines an own enumerable property even when the key is `__proto__`, which a
 * plain assignment would silently route to the prototype instead of the object.
 */
export declare function setKey(object: any, key: any, value: any): void;
