/**
 * Errors carry the 1-based source line so a decoder failure points at the row
 * that caused it. `line: 0` means "no line context" (encoder-side failures).
 */
/** A stable, coarse classification shared with the Rust decoder's `ErrorKind`. */
export type ToonErrorKind = 'syntax' | 'indentation' | 'length-mismatch' | 'duplicate-key' | 'depth-limit' | 'input-limit';
export declare class ToonDecodeError extends SyntaxError {
    readonly line?: number;
    /** 1-based column, when the decoder knows where on the line it failed. */
    readonly column?: number;
    readonly source?: string;
    readonly reason: string;
    readonly kind: ToonErrorKind;
    constructor(message: string, context?: {
        line?: number;
        column?: number;
        source?: string;
        cause?: unknown;
    });
}
/** Classifies a decoder reason; message wording may change, kinds do not. */
export declare function errorKind(reason: string): ToonErrorKind;
/**
 * Positioned error raised inside the decoder. `decode` re-raises it as a
 * [`ToonDecodeError`] at the public boundary; the streaming and TOONL entry
 * points surface it directly.
 */
export declare class ToonError extends SyntaxError {
    readonly line: number;
    readonly column?: number;
    readonly source?: string;
    readonly reason: string;
    readonly kind: ToonErrorKind;
    constructor(line: number, message: string, context?: {
        column?: number;
        source?: string;
        cause?: unknown;
    });
}
export declare class ToonlError extends Error {
    line: number;
    reason: string;
    constructor(line: number, message: string);
}
export declare class ToonlCursorInvalidationError extends ToonlError {
    condition: string;
    details: Record<string, unknown>;
    constructor(condition: string, message: string, details?: Record<string, unknown>);
}
export declare function toonError(line: number, message: string, context?: {
    column?: number;
    source?: string;
    cause?: unknown;
}): ToonError;
export declare function toonlError(line: number, message: string): ToonlError;
/** Re-raises a decoder error as a TOONL error, keeping line and reason. */
export declare function asToonlError(error: any): ToonlError;
