/**
 * MCP stdio transport: one JSON-RPC message per line.
 *
 * Framing follows the stdio binding of the pinned revision:
 *
 * - messages are read from stdin, one per line;
 * - messages are written to stdout, one per line, never containing an embedded
 *   newline;
 * - nothing that is not a valid MCP message reaches stdout, so logging goes to
 *   stderr;
 * - EOF on stdin is the graceful shutdown signal.
 */
import type { McpDispatcher } from './dispatcher.js';
export interface StdioStreams {
    input: NodeJS.ReadableStream;
    output: NodeJS.WritableStream;
}
/** Serve MCP over stdin/stdout until EOF. */
export declare function serveStdio(dispatcher: McpDispatcher): void;
/**
 * Serve MCP over arbitrary line-oriented streams.
 *
 * Resolves once the input stream ends, so tests can await a full transcript.
 * Responses are written in request order: each line is awaited before the next
 * is dispatched, so a slow call cannot reorder the stream.
 */
export declare function serveStdioWith(dispatcher: McpDispatcher, streams: StdioStreams): Promise<void>;
//# sourceMappingURL=stdio.d.ts.map