/**
 * Single-consumer document queue backing every duplex transport's receive
 * iterator: producers push complete documents, exactly one consumer drains
 * them, and end/fail settle the stream deterministically.
 */
export class DocumentQueue {
    items = [];
    waiter;
    ended = false;
    failure;
    consumed = false;
    push(document) {
        if (this.ended)
            return;
        if (this.waiter) {
            const waiter = this.waiter;
            this.waiter = undefined;
            waiter.resolve({ value: document, done: false });
            return;
        }
        this.items.push(document);
    }
    /** End the stream cleanly; queued documents are still delivered first. */
    end() {
        if (this.ended)
            return;
        this.ended = true;
        if (this.waiter && this.items.length === 0) {
            const waiter = this.waiter;
            this.waiter = undefined;
            waiter.resolve({ value: undefined, done: true });
        }
    }
    /** Fail the stream; queued documents are still delivered first. */
    fail(error) {
        if (this.ended)
            return;
        this.ended = true;
        this.failure = error;
        if (this.waiter && this.items.length === 0) {
            const waiter = this.waiter;
            this.waiter = undefined;
            waiter.reject(error);
        }
    }
    /**
     * The stream of documents. A supplied abort signal ends iteration promptly
     * without failing the transport. Only one consumer may ever attach.
     */
    iterate(options) {
        if (this.consumed) {
            throw new Error('TOON-RPC transport receive stream supports a single consumer');
        }
        this.consumed = true;
        const signal = options?.signal;
        const next = () => {
            if (signal?.aborted)
                return Promise.resolve({ value: undefined, done: true });
            const queued = this.items.shift();
            if (queued !== undefined)
                return Promise.resolve({ value: queued, done: false });
            if (this.failure)
                return Promise.reject(this.failure);
            if (this.ended)
                return Promise.resolve({ value: undefined, done: true });
            return new Promise((resolve, reject) => {
                this.waiter = { resolve, reject };
                signal?.addEventListener('abort', () => {
                    if (this.waiter) {
                        const waiter = this.waiter;
                        this.waiter = undefined;
                        waiter.resolve({ value: undefined, done: true });
                    }
                }, { once: true });
            });
        };
        return {
            [Symbol.asyncIterator]: () => ({
                next,
                return: () => Promise.resolve({ value: undefined, done: true }),
            }),
        };
    }
}
/** Race an operation against an abort signal without leaking listeners. */
export function raceSignal(operation, signal) {
    if (!signal)
        return operation;
    if (signal.aborted)
        return Promise.reject(abortError());
    return new Promise((resolve, reject) => {
        const onAbort = () => reject(abortError());
        signal.addEventListener('abort', onAbort, { once: true });
        operation.then((value) => {
            signal.removeEventListener('abort', onAbort);
            resolve(value);
        }, (error) => {
            signal.removeEventListener('abort', onAbort);
            reject(error);
        });
    });
}
export function abortError() {
    const error = new Error('TOON-RPC transport operation was aborted');
    error.name = 'TransportAbortError';
    return error;
}
export function asTransportError(error) {
    return error instanceof Error ? error : new Error(String(error));
}
//# sourceMappingURL=internal.js.map