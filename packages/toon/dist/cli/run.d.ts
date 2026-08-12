/**
 * The `toon` binary: a thin argv adapter over the canonical codec, implementing
 * the upstream `@toon-format/cli` contract so upstream scripts run unmodified.
 * `tq` keeps its jq-faithful flag vocabulary; this front-end keeps upstream's.
 */
import type { CliIo, InputSource } from './io.js';
/** Runs one CLI invocation and returns the process exit code. */
export declare function runCli(argv: readonly string[], io: CliIo): Promise<number>;
/**
 * Upstream detects the mode from the file extension and defaults to encode,
 * which is what a bare `cat data.json | toon` relies on.
 */
export declare function detectMode(input: InputSource, encodeFlag: boolean, decodeFlag: boolean): 'encode' | 'decode';
