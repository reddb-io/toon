import type { CoreValue, Id, Params, Response } from './protocol.js';
export * from './protocol.js';
export interface Transport {
    send(data: Uint8Array): Promise<void>;
    recv(): AsyncIterable<Uint8Array>;
    close(): Promise<void>;
}
export declare class RpcError extends Error {
    readonly code: number;
    readonly data: CoreValue | undefined;
    readonly hasData: boolean;
    constructor(code: number, message: string);
    constructor(code: number, message: string, data: CoreValue);
}
export interface MethodHandler {
    (params: Params | undefined, id: Id | undefined): Promise<CoreValue>;
}
export declare class Server {
    private methods;
    constructor();
    register(method: string, handler: MethodHandler): void;
    handle(raw: Uint8Array): Promise<Uint8Array>;
    handleText(text: string): Promise<Uint8Array>;
    /**
     * Dispatch one already-parsed request entry.
     *
     * Returns the response to send, or `undefined` for a valid notification.
     * Malformed entries always produce an uncorrelated Invalid Request response.
     */
    dispatchEntry(entry: unknown): Promise<Response | undefined>;
}
export declare class Client {
    private transport;
    private idCounter;
    private pending;
    constructor(transport: Transport);
    call(method: string, params: Params): Promise<CoreValue>;
    recv(): AsyncGenerator<never, void, unknown>;
    close(): Promise<void>;
}
export declare function createStdioTransport(): Transport;
//# sourceMappingURL=index.d.ts.map