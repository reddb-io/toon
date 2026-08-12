/**
 * Every byte the `toon` CLI reads or writes goes through this seam, so a test
 * can drive a whole run in-process while the real entry point binds it to
 * `process`.
 */
export interface CliIo {
    /** Resolves relative input, output, and label paths. */
    cwd: string;
    stdout(text: string): void;
    stderr(text: string): void;
    stdin(): AsyncIterable<Uint8Array | string>;
}
export type InputSource = {
    type: 'stdin';
} | {
    type: 'file';
    path: string;
};
/** Reads a whole input as text, replacing ill-formed bytes like Node's stdin does. */
export declare function readInput(source: InputSource, io: CliIo): Promise<string>;
/** Streams an input as lines. Strict decoding refuses to substitute U+FFFD. */
export declare function readLinesFromSource(source: InputSource, strict: boolean, io: CliIo): AsyncIterable<string>;
/** Writes the pieces to a file or to stdout, always ending with a newline. */
export declare function writeStream(pieces: AsyncIterable<string> | Iterable<string>, options: {
    outputPath?: string;
    separator: string;
    io: CliIo;
}): Promise<void>;
/** Names an input the way the upstream success lines do. */
export declare function formatInputLabel(source: InputSource, io: CliIo): string;
