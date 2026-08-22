import type { JsonValue } from '@reddb-io/toon';
export declare const TOONRPC_VERSION = "1.0";
export interface Request {
    toonrpc: '1.0';
    method: string;
    params: Params;
    id: Id;
}
export interface Notification {
    toonrpc: '1.0';
    method: string;
    params: Params;
}
export interface ResponseSuccess {
    toonrpc: '1.0';
    result: JsonValue;
    id: Id;
}
export interface ResponseError {
    toonrpc: '1.0';
    error: ErrorObject;
    id: Id;
}
/** A response is exactly one of success or error — never both, never neither. */
export type Response = ResponseSuccess | ResponseError;
export interface ErrorObject {
    code: number;
    message: string;
    data?: JsonValue;
}
export type Id = string | number | null;
export type Params = JsonValue[] | Record<string, JsonValue>;
export interface Transport {
    send(data: Uint8Array): Promise<void>;
    recv(): AsyncIterable<Uint8Array>;
    close(): Promise<void>;
}
export declare class RpcError extends Error {
    code: number;
    data?: JsonValue | undefined;
    constructor(code: number, message: string, data?: JsonValue | undefined);
}
export interface MethodHandler {
    (params: Params, id: Id): Promise<JsonValue>;
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
     * Returns the Response to send back, or `undefined` for a notification —
     * a request whose `id` is ABSENT. A present-but-`null` id is still an id
     * (discouraged, but legal), so it earns a response. Notifications run their
     * handler; only the answer is withheld, and a notification for an unknown
     * method or a throwing handler is dropped silently, as the spec requires.
     */
    dispatchEntry(entry: unknown): Promise<Response | undefined>;
}
export declare class Client {
    private transport;
    private idCounter;
    private pending;
    constructor(transport: Transport);
    call(method: string, params: Params): Promise<JsonValue>;
    recv(): AsyncGenerator<never, void, unknown>;
    close(): Promise<void>;
}
export declare function createStdioTransport(): Transport;
//# sourceMappingURL=index.d.ts.map