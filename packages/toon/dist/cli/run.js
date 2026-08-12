/**
 * The `toon` binary: a thin argv adapter over the canonical codec, implementing
 * the upstream `@toon-format/cli` contract so upstream scripts run unmodified.
 * `tq` keeps its jq-faithful flag vocabulary; this front-end keeps upstream's.
 */
import * as path from 'node:path';
import { DELIMITERS } from '../constants.js';
import { VERSION } from '../version.js';
import { HELP_TEXT, parseCliArgs } from './args.js';
import { decodeToJson, encodeToToon } from './conversion.js';
import { CliError, formatReport } from './errors.js';
/** Runs one CLI invocation and returns the process exit code. */
export async function runCli(argv, io) {
    let verbose = false;
    try {
        const args = parseCliArgs(argv);
        verbose = args.verbose;
        if (args.help) {
            io.stdout(HELP_TEXT);
            return 0;
        }
        if (args.version) {
            io.stdout(`${VERSION}\n`);
            return 0;
        }
        const input = !args.input || args.input === '-'
            ? { type: 'stdin' }
            : { type: 'file', path: path.resolve(io.cwd, args.input) };
        const output = args.output ? path.resolve(io.cwd, args.output) : undefined;
        const indentSize = Number.parseInt(args.indent || '2', 10);
        if (Number.isNaN(indentSize) || indentSize < 0) {
            throw new CliError(`Invalid indent value: ${args.indent}`);
        }
        const delimiter = resolveDelimiter(args.delimiter);
        if (detectMode(input, args.encode, args.decode) === 'encode') {
            await encodeToToon({
                input,
                output,
                delimiter,
                indentSize,
                shouldPrintStats: args.stats,
                io,
            });
        }
        else {
            await decodeToJson({ input, output, indentSize, strict: args.strict, io });
        }
        return 0;
    }
    catch (error) {
        io.stderr(`✖ ${formatReport(error, verbose)}\n`);
        return 1;
    }
}
/**
 * Upstream detects the mode from the file extension and defaults to encode,
 * which is what a bare `cat data.json | toon` relies on.
 */
export function detectMode(input, encodeFlag, decodeFlag) {
    if (encodeFlag)
        return 'encode';
    if (decodeFlag)
        return 'decode';
    if (input.type === 'file') {
        if (input.path.endsWith('.json'))
            return 'encode';
        if (input.path.endsWith('.toon'))
            return 'decode';
    }
    return 'encode';
}
/** Accepts the upstream literals plus the readable names `tq` already takes. */
function resolveDelimiter(value) {
    if (!value)
        return DELIMITERS.comma;
    const named = DELIMITERS[value];
    if (named)
        return named;
    if (value === '\\t')
        return DELIMITERS.tab;
    if (value === DELIMITERS.comma || value === DELIMITERS.tab || value === DELIMITERS.pipe) {
        return value;
    }
    throw new CliError(`Invalid delimiter ${JSON.stringify(value)}. `
        + 'Valid delimiters are: comma (,), tab (\\t), pipe (|)');
}
