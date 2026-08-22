/**
 * Multi-protocol RPC: auto-detects JSON-RPC 2.0 vs TOON-RPC 1.0 on the wire
 * and answers in the same format the client used.
 */
import type { JsonValue } from '@reddb-io/toon';
import { Server } from '@reddb-io/toon-rpc';
import type { Id, Params } from '@reddb-io/toon-rpc';
export { Server } from '@reddb-io/toon-rpc';
export declare const JSONRPC_VERSION = "2.0";
/** Wire protocol variants the dispatcher can negotiate. */
export type Protocol = 'jsonrpc' | 'toonrpc';
/** MIME type for HTTP `Content-Type` / `Accept` negotiation. */
export declare function contentTypeFor(protocol: Protocol): string;
/** Detect the protocol from a content-type hint and/or raw bytes. */
export declare function detectProtocol(raw: Uint8Array | string, contentType?: string): Protocol;
/** A single method registry behind JSON-RPC and TOON-RPC wire formats. */
export declare class MultiRpc {
    private server;
    constructor(server: Server);
    handle(raw: Uint8Array | string, contentType?: string): Promise<Uint8Array>;
    handleWithProtocol(raw: Uint8Array | string, contentType?: string): Promise<{
        protocol: Protocol;
        body: Uint8Array;
    }>;
    private handleJsonRpc;
    private dispatchJsonRpcEntry;
}
/** Re-encode an already-parsed RPC message in the named dialect. */
export declare function encodeMessage(message: Record<string, JsonValue>, protocol: Protocol): string;
/** Decode a framed message into the JSON-RPC-compatible object shape. */
export declare function decodeMessage(frame: string): {
    message: Record<string, JsonValue>;
    protocol: Protocol;
};
export type { Id, Params };
//# sourceMappingURL=index.d.ts.map