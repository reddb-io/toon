/**
 * Errors carry the 1-based source line so a decoder failure points at the row
 * that caused it. `line: 0` means "no line context" (encoder-side failures).
 */
export declare class ToonDecodeError extends SyntaxError {
    readonly line?: number;
    readonly source?: string;
    readonly reason: string;
    constructor(message: string, context?: {
        line?: number;
        source?: string;
        cause?: unknown;
    });
}
/** Error used by the explicit pre-v4 compatibility codec. */
export declare class ToonError extends SyntaxError {
    readonly line: number;
    readonly source?: string;
    readonly reason: string;
    constructor(line: number, message: string, context?: {
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
    source?: string;
    cause?: unknown;
}): ToonError;
export declare function toonlError(line: number, message: string): ToonlError;
/** Re-raises a decoder error as a TOONL error, keeping line and reason. */
export declare function asToonlError(error: any): ToonlError;
