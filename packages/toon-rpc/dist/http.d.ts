/**
 * HTTP transport for TOON-RPC.
 *
 * HTTP is a request/response exchange, not a duplex stream: each POST carries
 * one complete RPC document and directly owns zero or one response document.
 * A notification-only exchange comes back with status 204 (or an empty body)
 * and produces no response document.
 */
import type { RequestResponseTransport, TransportOperationOptions } from './transport.js';
export declare const TOON_RPC_CONTENT_TYPE = "application/toon";
export interface HttpTransportOptions {
    url: string | URL;
    headers?: Record<string, string>;
    /** Injectable fetch implementation; defaults to the global fetch. */
    fetch?: typeof fetch;
}
export declare class HttpTransportError extends Error {
    readonly status: number;
    constructor(status: number, statusText: string);
}
export declare class HttpTransport implements RequestResponseTransport {
    readonly kind: "request-response";
    private readonly url;
    private readonly headers;
    private readonly fetchImpl;
    private readonly lifetime;
    private closed;
    constructor(options: HttpTransportOptions);
    request(document: Uint8Array, options?: TransportOperationOptions): Promise<Uint8Array | undefined>;
    close(): Promise<void>;
}
export declare function createHttpTransport(options: HttpTransportOptions): HttpTransport;
//# sourceMappingURL=http.d.ts.map