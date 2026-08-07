/**
 * Errors carry the 1-based source line so a decoder failure points at the row
 * that caused it. `line: 0` means "no line context" (encoder-side failures).
 */
export class ToonDecodeError extends SyntaxError {
    line;
    source;
    reason;
    constructor(message, context = {}) {
        const prefix = context.line === undefined || context.line === 0 ? '' : `Line ${context.line}: `;
        super(prefix + message, context.cause === undefined ? undefined : { cause: context.cause });
        this.name = 'ToonDecodeError';
        this.line = context.line;
        this.source = context.source;
        this.reason = message;
    }
}
/** Error used by the explicit pre-v4 compatibility codec. */
export class ToonError extends SyntaxError {
    line;
    source;
    reason;
    constructor(line, message, context = {}) {
        super(line === 0 ? message : `line ${line}: ${message}`, context.cause === undefined ? undefined : { cause: context.cause });
        this.name = 'ToonError';
        this.line = line;
        this.source = context.source;
        this.reason = message;
    }
}
export class ToonlError extends Error {
    line;
    reason;
    constructor(line, message) {
        super(line === 0 ? message : `line ${line}: ${message}`);
        this.name = 'ToonlError';
        this.line = line;
        this.reason = message;
    }
}
export class ToonlCursorInvalidationError extends ToonlError {
    condition;
    details;
    constructor(condition, message, details = {}) {
        super(0, message);
        this.name = 'ToonlCursorInvalidationError';
        this.condition = condition;
        this.details = details;
    }
}
export function toonError(line, message, context = {}) {
    return new ToonError(line, message, context);
}
export function toonlError(line, message) {
    return new ToonlError(line, message);
}
/** Re-raises a decoder error as a TOONL error, keeping line and reason. */
export function asToonlError(error) {
    if (error instanceof ToonlError) {
        return error;
    }
    if (error instanceof ToonError) {
        return new ToonlError(error.line, error.reason);
    }
    return new ToonlError(0, String(error && error.message ? error.message : error));
}
