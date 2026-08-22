/**
 * WebSocket transport for TOON-RPC
 */
import type { Transport } from './index';
export declare function createWebSocketTransport(url: string): Transport;
/**
 * Node.js WebSocket transport using `ws` package
 */
export declare class NodeWebSocketClient {
    private url;
    private ws;
    private queue;
    private waiters;
    constructor(url: string);
    connect(): Promise<void>;
    send(data: Uint8Array): Promise<void>;
    recv(): Promise<Uint8Array>;
    close(): Promise<void>;
}
//# sourceMappingURL=websocket.d.ts.map