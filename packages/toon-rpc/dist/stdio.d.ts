/**
 * stdio transport for TOON-RPC (Node.js).
 *
 * A stdin/stdout pipe is a byte stream, so this transport speaks the
 * length-prefixed stream framing profile from `framing.ts` — one frame per
 * complete RPC document, reassembled across arbitrary chunk splits. The
 * streams are injectable for tests and for embedding a child process's
 * pipes; they default to this process's stdin/stdout.
 */
import type { Readable, Writable } from 'node:stream';
import type { DuplexTransport, TransportOperationOptions } from './transport.js';
export interface StdioTransportOptions {
    input?: Readable;
    output?: Writable;
}
export declare class StdioTransport implements DuplexTransport {
    readonly kind: "duplex";
    private readonly input;
    private readonly output;
    private readonly documents;
    private readonly decoder;
    private started;
    private closed;
    private failure;
    constructor(options?: StdioTransportOptions);
    open(): Promise<void>;
    send(document: Uint8Array, options?: TransportOperationOptions): Promise<void>;
    receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array>;
    close(): Promise<void>;
    private failWith;
}
export declare function createStdioTransport(options?: StdioTransportOptions): StdioTransport;
//# sourceMappingURL=stdio.d.ts.map