import type { CoreValue, Id, Params } from './protocol.js';
import type { ClientTransport } from './transport.js';
export type ClientStatus = 'idle' | 'opening' | 'open' | 'closed' | 'failed';
export type ClientDiagnosticReason = 'parse-error' | 'invalid-response' | 'unknown-id' | 'duplicate-id';
export interface ClientDiagnostic {
    reason: ClientDiagnosticReason;
    index?: number;
    id?: Id;
    error?: unknown;
}
export interface ClientOptions {
    onDiagnostic?: (diagnostic: ClientDiagnostic) => void;
}
export interface CallOptions {
    id?: Id;
    signal?: AbortSignal;
    timeoutMs?: number;
}
export interface NotifyOptions {
    signal?: AbortSignal;
    timeoutMs?: number;
}
export declare class ClientClosedError extends Error {
    constructor(message?: string);
}
export declare class ClientAbortError extends Error {
    constructor();
}
export declare class ClientTimeoutError extends Error {
    constructor(timeoutMs: number);
}
export declare class ClientProtocolError extends Error {
    constructor(message: string);
}
export declare class Client {
    private readonly transport;
    private readonly options;
    private idCounter;
    private readonly pending;
    private state;
    private terminalError;
    private readonly terminalController;
    private resolveTermination;
    private readonly termination;
    private startPromise;
    private receivePromise;
    private transportClosePromise;
    private closePromise;
    constructor(transport: ClientTransport, options?: ClientOptions);
    get status(): ClientStatus;
    get pendingCallCount(): number;
    start(): Promise<void>;
    call(method: string, params?: Params, options?: CallOptions): Promise<CoreValue>;
    notify(method: string, params?: Params, options?: NotifyOptions): Promise<void>;
    close(): Promise<void>;
    private dispatchCall;
    private ensureOpen;
    private receiveLoop;
    private processDocument;
    private settleResponse;
    private rejectPending;
    private takePending;
    private rejectAll;
    private terminate;
    private closeTransport;
    private assertOpen;
    private diagnostic;
    private allocateId;
}
//# sourceMappingURL=client.d.ts.map