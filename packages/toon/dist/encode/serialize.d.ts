import { type EncodeReplacer } from './replacer.js';
export interface EncodeOptions {
    delimiter?: ',' | '|' | '\t';
    indentSize?: number;
    /** @deprecated Use indentSize. */
    indent?: number;
    replacer?: EncodeReplacer;
    cyclicDiscriminatedArrays?: boolean;
    primitiveArrayColumns?: boolean;
    objectArrayColumns?: boolean;
    maxDepth?: number;
}
/** Encodes normalized JSON using the canonical v4.1 forms. */
export declare function encode(input: unknown, options?: EncodeOptions): string;
/** Encodes normalized JSON as TOON lines without trailing newlines. */
export declare function encodeLines(input: unknown, options?: EncodeOptions): Iterable<string>;
