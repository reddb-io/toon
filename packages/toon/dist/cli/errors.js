/**
 * The `toon` CLI error boundary, mirroring the upstream `@toon-format/cli`
 * presentation: a condition the CLI recognized and phrased for a human prints
 * one clean line; anything else is a defect in the tool and prints its stack
 * unasked. `--verbose` adds the cause chain and stack to both.
 */
import { ToonDecodeError, ToonError } from '../errors.js';
import { formatDecodeError } from './format-error.js';
/** Raised for a condition the CLI recognized and phrased for a human. */
export class CliError extends Error {
    constructor(message, options) {
        super(message, options);
        this.name = 'CliError';
    }
}
/** Renders a recognized error for a human — the boundary appends the stack itself. */
export function describeError(error) {
    if (isPositionedDecodeError(error))
        return formatDecodeError(error);
    return error instanceof Error ? error.message : String(error);
}
/**
 * Reports whether the CLI raised this error deliberately rather than tripping
 * over it. A Node system error carries a string `code` and reaches the boundary
 * as the honest answer to what the user asked for, so it reads as deliberate too.
 */
export function isExpectedError(error) {
    if (error instanceof CliError)
        return true;
    if (error instanceof ToonDecodeError || error instanceof ToonError)
        return true;
    return error instanceof Error && typeof error.code === 'string';
}
/** Builds the stderr body for a failed run, without the `✖ ` prefix. */
export function formatReport(error, isVerbose) {
    const sections = [describeError(error)];
    if (isVerbose || !isExpectedError(error)) {
        const causeChain = formatCauseChain(error);
        if (causeChain)
            sections.push(causeChain);
        if (error instanceof Error && error.stack)
            sections.push(error.stack);
    }
    return sections.join('\n\n');
}
function isPositionedDecodeError(error) {
    return ((error instanceof ToonDecodeError || error instanceof ToonError)
        && error.line !== undefined);
}
function formatCauseChain(error) {
    const causeLines = [];
    let current = error instanceof Error ? error.cause : undefined;
    while (current instanceof Error) {
        causeLines.push(`Caused by: ${current.name || 'Error'}: ${current.message}`);
        current = current.cause;
    }
    return causeLines.join('\n');
}
