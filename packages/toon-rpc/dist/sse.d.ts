/**
 * Server-Sent Events (SSE) transport for TOON-RPC
 */
export declare class SseClient {
    private url;
    private eventSource;
    private queue;
    private waiters;
    constructor(url: string);
    connect(): Promise<void>;
    send(data: Uint8Array): Promise<void>;
    recv(): Promise<Uint8Array>;
    close(): Promise<void>;
}
//# sourceMappingURL=sse.d.ts.map