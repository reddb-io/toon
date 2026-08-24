export interface TransportOperationOptions {
    signal?: AbortSignal;
}
interface TransportLifecycle {
    open?(options?: TransportOperationOptions): Promise<void>;
    close(): Promise<void>;
}
/** A framed transport that yields exactly one complete RPC document per item. */
export interface DuplexTransport extends TransportLifecycle {
    readonly kind: 'duplex';
    send(document: Uint8Array, options?: TransportOperationOptions): Promise<void>;
    receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array>;
}
/** A transport where each request directly owns its optional response document. */
export interface RequestResponseTransport extends TransportLifecycle {
    readonly kind: 'request-response';
    request(document: Uint8Array, options?: TransportOperationOptions): Promise<Uint8Array | undefined>;
}
export type ClientTransport = DuplexTransport | RequestResponseTransport;
export {};
//# sourceMappingURL=transport.d.ts.map