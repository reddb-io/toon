/**
 * TCP transport for TOON-RPC (Node.js)
 */
export declare class TcpClient {
    private host;
    private port;
    private socket;
    private buffer;
    constructor(host: string, port: number);
    connect(): Promise<void>;
    send(data: Uint8Array): Promise<void>;
    recv(): Promise<Uint8Array>;
    close(): Promise<void>;
}
//# sourceMappingURL=tcp.d.ts.map