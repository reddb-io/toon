/**
 * Errors carry the 1-based source line so a decoder failure points at the row
 * that caused it. `line: 0` means "no line context" (encoder-side failures).
 */
export class ToonDecodeError extends SyntaxError {
    line;
    /** 1-based column, when the decoder knows where on the line it failed. */
    column;
    source;
    reason;
    kind;
    constructor(message, context = {}) {
        const prefix = context.line === undefined || context.line === 0 ? '' : `Line ${context.line}: `;
        super(prefix + message, context.cause === undefined ? undefined : { cause: context.cause });
        this.name = 'ToonDecodeError';
        this.line = context.line;
        this.column = context.column;
        this.source = context.source;
        this.reason = message;
        this.kind = errorKind(message);
    }
}
/** Classifies a decoder reason; message wording may change, kinds do not. */
export function errorKind(reason) {
    if (/^(over-indented line|invalid indentation|tab used as indentation)$/.test(reason))
        return 'indentation';
    if (/length mismatch|count mismatch|^expected \d+ .*, but got \d+$/.test(reason))
        return 'length-mismatch';
    if (/^duplicate (object key|field name in header)$/.test(reason))
        return 'duplicate-key';
    if (reason.startsWith('maximum nesting depth exceeded'))
        return 'depth-limit';
    if (/ exceeds max(InputBytes|ArrayLength|Keys) \(/.test(reason))
        return 'input-limit';
    return 'syntax';
}
/**
 * Positioned error raised inside the decoder. `decode` re-raises it as a
 * [`ToonDecodeError`] at the public boundary; the streaming and TOONL entry
 * points surface it directly.
 */
export class ToonError extends SyntaxError {
    line;
    column;
    source;
    reason;
    kind;
    constructor(line, message, context = {}) {
        super(line === 0 ? message : `line ${line}: ${message}`, context.cause === undefined ? undefined : { cause: context.cause });
        this.name = 'ToonError';
        this.line = line;
        this.column = context.column;
        this.source = context.source;
        this.reason = message;
        this.kind = errorKind(message);
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
