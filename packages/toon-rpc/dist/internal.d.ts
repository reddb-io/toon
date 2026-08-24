import type { TransportOperationOptions } from './transport.js';
/**
 * Single-consumer document queue backing every duplex transport's receive
 * iterator: producers push complete documents, exactly one consumer drains
 * them, and end/fail settle the stream deterministically.
 */
export declare class DocumentQueue {
    private readonly items;
    private waiter;
    private ended;
    private failure;
    private consumed;
    push(document: Uint8Array): void;
    /** End the stream cleanly; queued documents are still delivered first. */
    end(): void;
    /** Fail the stream; queued documents are still delivered first. */
    fail(error: Error): void;
    /**
     * The stream of documents. A supplied abort signal ends iteration promptly
     * without failing the transport. Only one consumer may ever attach.
     */
    iterate(options?: TransportOperationOptions): AsyncIterable<Uint8Array>;
}
/** Race an operation against an abort signal without leaking listeners. */
export declare function raceSignal<T>(operation: Promise<T>, signal?: AbortSignal): Promise<T>;
export declare function abortError(): Error;
export declare function asTransportError(error: unknown): Error;
//# sourceMappingURL=internal.d.ts.map