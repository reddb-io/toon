/**
 * The two conversions the `toon` CLI performs, over the canonical event-stream
 * codec: JSON to TOON and TOON back to JSON. Results go to stdout or to
 * `--output`; every diagnostic goes to stderr, so a pipeline stays clean.
 */
import { type CliIo, type InputSource } from './io.js';
export interface ConversionConfig {
    input: InputSource;
    output?: string;
    indentSize: number;
    io: CliIo;
}
export declare function encodeToToon(config: ConversionConfig & {
    delimiter: ',' | '|' | '\t';
    shouldPrintStats: boolean;
}): Promise<void>;
export declare function decodeToJson(config: ConversionConfig & {
    strict: boolean;
}): Promise<void>;
