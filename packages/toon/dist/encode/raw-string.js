const COMMENT_LINE_PATTERN = /(?:^\uFEFF?|\n) *#/;
/** A primitive token that the encoder emits verbatim. */
export class RawString {
    value;
    constructor(value) {
        if (COMMENT_LINE_PATTERN.test(value)) {
            throw new TypeError(`Raw string must not contain a line starting with "#": ${JSON.stringify(value)}`);
        }
        this.value = value;
    }
}
export function rawString(value) {
    return new RawString(value);
}
export function isRawString(value) {
    return value instanceof RawString;
}
