/**
 * TCP transport for TOON-RPC (Node.js).
 *
 * TCP is a byte stream with no document boundaries of its own, so this
 * transport speaks the length-prefixed stream framing profile from
 * `framing.ts` — never newline inference. Each sent document becomes one
 * frame; received bytes are reassembled into complete documents across
 * arbitrary chunk splits.
 */
import * as net from 'node:net';
import type { DuplexTransport, TransportOperationOptions } from './transport.js';
export interface TcpTransportOptions {
    host?: string;
    port?: number;
    /** Injectable socket factory; defaults to net.createConnection(port, host). */
    connect?: () => net.Socket;
}
export declare class TcpTransport implements DuplexTransport {
    readonly kind: "duplex";
    private readonly options;
    private readonly documents;
    private readonly decoder;
    private socket;
    private openPromise;
    private closePromise;
    private failure;
    constructor(options: TcpTransportOptions);
    open(options?: TransportOperationOptions): Promise<void>;
    send(document: Uint8Array, options?: TransportOperationOptions): Promise<void>;
    receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array>;
    close(): Promise<void>;
    private connect;
    private failWith;
}
export declare function createTcpTransport(options: TcpTransportOptions): TcpTransport;
//# sourceMappingURL=tcp.d.ts.map