/**
 * Server-Sent Events transport for TOON-RPC.
 *
 * SSE composes a duplex profile out of two HTTP legs: documents travel to the
 * server as POST bodies, and documents travel back as complete `data:` events
 * on one long-lived event stream. A multi-line TOON document arrives as one
 * event whose data lines rejoin with LF exactly as the SSE specification
 * defines, so document boundaries are the event boundaries.
 *
 * Built on fetch streaming rather than EventSource: EventSource cannot POST,
 * cannot send headers, and cannot abort deterministically.
 */
import type { DuplexTransport, TransportOperationOptions } from './transport.js';
export interface SseTransportOptions {
    /** The event-stream URL documents are received from. */
    url: string | URL;
    /** The URL documents are POSTed to; defaults to `url`. */
    postUrl?: string | URL;
    headers?: Record<string, string>;
    /** Injectable fetch implementation; defaults to the global fetch. */
    fetch?: typeof fetch;
}
export declare class SseTransportError extends Error {
    readonly status: number;
    constructor(status: number, statusText: string);
}
export declare class SseTransport implements DuplexTransport {
    readonly kind: "duplex";
    private readonly url;
    private readonly postUrl;
    private readonly headers;
    private readonly fetchImpl;
    private readonly documents;
    private readonly lifetime;
    private openPromise;
    private pumpPromise;
    private closed;
    private failure;
    constructor(options: SseTransportOptions);
    open(options?: TransportOperationOptions): Promise<void>;
    send(document: Uint8Array, options?: TransportOperationOptions): Promise<void>;
    receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array>;
    close(): Promise<void>;
    private connect;
    private pump;
}
export declare function createSseTransport(options: SseTransportOptions): SseTransport;
//# sourceMappingURL=sse.d.ts.map