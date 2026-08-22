/**
 * Multi-protocol RPC — auto-detects JSON-RPC 2.0 vs TOON-RPC 1.0 on the wire
 * and answers in the same format the client used.
 *
 * TypeScript port of the Rust `reddb_io_toon_rpc::multi` module; the detection
 * rules are the same on both sides so a mixed fleet cannot disagree about what
 * a request was:
 *
 * - An explicit `Content-Type: application/json` or `application/toon` wins.
 * - A body starting with `{` or `[` whose head contains `"jsonrpc"` is JSON-RPC.
 * - A body starting with `toonrpc:` or `{toonrpc` is TOON-RPC.
 * - Anything else is TOON-RPC — the preferred format.
 */
import type { JsonValue } from '@reddb-io/toon';
import { Server } from './index.js';
import type { Id, Params } from './index.js';
export declare const JSONRPC_VERSION = "2.0";
/** Wire protocol variants the dispatcher can negotiate. */
export type Protocol = 'jsonrpc' | 'toonrpc';
/** MIME type for HTTP `Content-Type` / `Accept` negotiation. */
export declare function contentTypeFor(protocol: Protocol): string;
/**
 * Detect the protocol from a content-type hint and/or raw bytes.
 *
 * An explicit content-type hint (when provided) wins over byte sniffing.
 */
export declare function detectProtocol(raw: Uint8Array | string, contentType?: string): Protocol;
/**
 * Multi-protocol dispatcher — a single method registry behind two wire formats.
 *
 * `handle` detects the dialect of each request and answers in kind, so a
 * JSON-RPC client and a TOON-RPC client can share one endpoint without either
 * being told about the other.
 */
export declare class MultiRpc {
    private server;
    constructor(server: Server);
    handle(raw: Uint8Array | string, contentType?: string): Promise<Uint8Array>;
    /**
     * Handle a request, returning the detected protocol alongside the response
     * bytes — for transports that need to set the right `Content-Type`.
     */
    handleWithProtocol(raw: Uint8Array | string, contentType?: string): Promise<{
        protocol: Protocol;
        body: Uint8Array;
    }>;
    private handleJsonRpc;
    /** Dispatch one JSON-RPC entry; `undefined` means notification, no reply. */
    private dispatchJsonRpcEntry;
}
/**
 * Re-encode one already-parsed RPC message in the named dialect.
 *
 * The envelope field travels with the dialect: `jsonrpc: "2.0"` on the JSON
 * wire, `toonrpc: "1.0"` on the TOON wire. Everything else is preserved.
 */
export declare function encodeMessage(message: Record<string, JsonValue>, protocol: Protocol): string;
/**
 * Read one already-framed message in either dialect into a plain object with a
 * `jsonrpc: "2.0"` envelope — the shape JSON-RPC consumers (e.g. an ACP
 * connection) already understand. Returns the message and the dialect it wore.
 */
export declare function decodeMessage(frame: string): {
    message: Record<string, JsonValue>;
    protocol: Protocol;
};
export type { Id, Params };
//# sourceMappingURL=multi.d.ts.map