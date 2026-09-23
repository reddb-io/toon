/**
 * The MCP stdio transport: newline-delimited JSON-RPC messages on stdin and
 * stdout, one message per line with no embedded newlines. Anything else the
 * server prints must go to stderr.
 *
 * Requests are answered concurrently, as MCP allows, with at most
 * `maxPendingCalls` in flight; past that, and past a line longer than
 * `maxFrameBytes`, the stream is ended with a JSON-RPC error.
 */
import type { Readable, Writable } from 'node:stream';
import type { Limits } from '@reddb-io/toon-rpc';
import type { McpService } from './index.js';
export interface StdioOptions {
    input?: Readable;
    output?: Writable;
    limits?: Partial<Pick<Limits, 'maxFrameBytes' | 'maxPendingCalls'>>;
}
/** Serve one MCP session over stdio; resolves once input ends and every answer is written. */
export declare function serveStdio(service: McpService, options?: StdioOptions): Promise<void>;
//# sourceMappingURL=stdio.d.ts.map