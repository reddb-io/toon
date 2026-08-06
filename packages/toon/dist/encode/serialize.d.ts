import { type EncodeReplacer } from './replacer.js';
export interface EncodeOptions {
    delimiter?: ',' | '|' | '\t';
    indentSize?: number;
    /** @deprecated Use indentSize. */
    indent?: number;
    replacer?: EncodeReplacer;
}
/** Encodes normalized JSON using the canonical v4.1 forms. */
export declare function encode(input: unknown, options?: EncodeOptions): string;
