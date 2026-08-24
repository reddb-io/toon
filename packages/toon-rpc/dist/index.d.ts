import type { CoreValue, Id, Params, Response } from './protocol.js';
export * from './protocol.js';
export * from './client.js';
export * from './rpc-error.js';
export * from './transport.js';
export * from './framing.js';
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
//# sourceMappingURL=index.d.ts.map