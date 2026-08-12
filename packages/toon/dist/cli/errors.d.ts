/**
 * The `toon` CLI error boundary, mirroring the upstream `@toon-format/cli`
 * presentation: a condition the CLI recognized and phrased for a human prints
 * one clean line; anything else is a defect in the tool and prints its stack
 * unasked. `--verbose` adds the cause chain and stack to both.
 */
/** Raised for a condition the CLI recognized and phrased for a human. */
export declare class CliError extends Error {
    constructor(message: string, options?: {
        cause?: unknown;
    });
}
/** Renders a recognized error for a human — the boundary appends the stack itself. */
export declare function describeError(error: unknown): string;
/**
 * Reports whether the CLI raised this error deliberately rather than tripping
 * over it. A Node system error carries a string `code` and reaches the boundary
 * as the honest answer to what the user asked for, so it reads as deliberate too.
 */
export declare function isExpectedError(error: unknown): boolean;
/** Builds the stderr body for a failed run, without the `✖ ` prefix. */
export declare function formatReport(error: unknown, isVerbose: boolean): string;
