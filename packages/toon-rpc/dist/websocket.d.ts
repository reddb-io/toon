/**
 * WebSocket transport for TOON-RPC.
 *
 * WebSocket is a framed duplex transport: every frame carries exactly one
 * complete RPC document. Text frames are decoded as UTF-8 documents; binary
 * frames are documents as-is; any other payload kind is a deterministic
 * transport failure, never a silent drop.
 *
 * The same class drives a browser `WebSocket` and the `ws` package's
 * `WebSocket` — both expose the addEventListener surface this transport
 * codes against. Pass the implementation via `options.webSocket` where the
 * global is absent (Node below 22, or to inject a test double).
 */
import type { DuplexTransport, TransportOperationOptions } from './transport.js';
interface WebSocketLike {
    readonly readyState: number;
    binaryType: string;
    send(data: Uint8Array): void;
    close(code?: number, reason?: string): void;
    addEventListener(type: string, listener: (event: never) => void, options?: {
        once?: boolean;
    }): void;
}
interface WebSocketConstructor {
    new (url: string): WebSocketLike;
    readonly OPEN?: number;
}
export interface WebSocketTransportOptions {
    url: string | URL;
    /** WebSocket implementation; defaults to the global WebSocket. */
    webSocket?: WebSocketConstructor;
}
export declare class WebSocketTransport implements DuplexTransport {
    readonly kind: "duplex";
    private readonly url;
    private readonly implementation;
    private readonly documents;
    private socket;
    private openPromise;
    private closePromise;
    private resolveClosed;
    private readonly closed;
    private failure;
    constructor(options: WebSocketTransportOptions);
    open(options?: TransportOperationOptions): Promise<void>;
    send(document: Uint8Array, options?: TransportOperationOptions): Promise<void>;
    receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array>;
    close(): Promise<void>;
    private connect;
    private failWith;
}
export declare function createWebSocketTransport(options: WebSocketTransportOptions): WebSocketTransport;
/** Node convenience: a transport backed by the `ws` package's WebSocket. */
export declare function createNodeWebSocketTransport(url: string | URL): Promise<WebSocketTransport>;
export {};
//# sourceMappingURL=websocket.d.ts.map