import type { TransportOperationOptions } from './transport.js';

/**
 * Single-consumer document queue backing every duplex transport's receive
 * iterator: producers push complete documents, exactly one consumer drains
 * them, and end/fail settle the stream deterministically.
 */
export class DocumentQueue {
  private readonly items: Uint8Array[] = [];
  private waiter:
    | { resolve: (result: IteratorResult<Uint8Array>) => void; reject: (error: Error) => void }
    | undefined;
  private ended = false;
  private failure: Error | undefined;
  private consumed = false;

  push(document: Uint8Array): void {
    if (this.ended) return;
    if (this.waiter) {
      const waiter = this.waiter;
      this.waiter = undefined;
      waiter.resolve({ value: document, done: false });
      return;
    }
    this.items.push(document);
  }

  /** End the stream cleanly; queued documents are still delivered first. */
  end(): void {
    if (this.ended) return;
    this.ended = true;
    if (this.waiter && this.items.length === 0) {
      const waiter = this.waiter;
      this.waiter = undefined;
      waiter.resolve({ value: undefined, done: true });
    }
  }

  /** Fail the stream; queued documents are still delivered first. */
  fail(error: Error): void {
    if (this.ended) return;
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
  iterate(options?: TransportOperationOptions): AsyncIterable<Uint8Array> {
    if (this.consumed) {
      throw new Error('TOON-RPC transport receive stream supports a single consumer');
    }
    this.consumed = true;
    const signal = options?.signal;

    const next = (): Promise<IteratorResult<Uint8Array>> => {
      if (signal?.aborted) return Promise.resolve({ value: undefined, done: true });
      const queued = this.items.shift();
      if (queued !== undefined) return Promise.resolve({ value: queued, done: false });
      if (this.failure) return Promise.reject(this.failure);
      if (this.ended) return Promise.resolve({ value: undefined, done: true });
      return new Promise<IteratorResult<Uint8Array>>((resolve, reject) => {
        this.waiter = { resolve, reject };
        signal?.addEventListener(
          'abort',
          () => {
            if (this.waiter) {
              const waiter = this.waiter;
              this.waiter = undefined;
              waiter.resolve({ value: undefined, done: true });
            }
          },
          { once: true }
        );
      });
    };

    return {
      [Symbol.asyncIterator]: () => ({
        next,
        return: () => Promise.resolve({ value: undefined, done: true as const }),
      }),
    };
  }
}

/** Race an operation against an abort signal without leaking listeners. */
export function raceSignal<T>(operation: Promise<T>, signal?: AbortSignal): Promise<T> {
  if (!signal) return operation;
  if (signal.aborted) return Promise.reject(abortError());
  return new Promise<T>((resolve, reject) => {
    const onAbort = () => reject(abortError());
    signal.addEventListener('abort', onAbort, { once: true });
    operation.then(
      (value) => {
        signal.removeEventListener('abort', onAbort);
        resolve(value);
      },
      (error) => {
        signal.removeEventListener('abort', onAbort);
        reject(error);
      }
    );
  });
}

export function abortError(): Error {
  const error = new Error('TOON-RPC transport operation was aborted');
  error.name = 'TransportAbortError';
  return error;
}

export function asTransportError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}
