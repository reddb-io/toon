/** A primitive token that the encoder emits verbatim. */
export declare class RawString {
    readonly value: string;
    constructor(value: string);
}
export declare function rawString(value: string): RawString;
export declare function isRawString(value: unknown): value is RawString;
