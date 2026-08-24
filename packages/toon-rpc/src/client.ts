import { decode, encode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';
import {
  TOONRPC_VERSION,
  isId,
  snapshotRequestObject,
  snapshotResponse,
} from './protocol.js';
import type { CoreValue, Id, Params, Response } from './protocol.js';
import type { ClientTransport, TransportOperationOptions } from './transport.js';
import { RpcError } from './rpc-error.js';

export type ClientStatus = 'idle' | 'opening' | 'open' | 'closed' | 'failed';
export type ClientDiagnosticReason =
  | 'parse-error'
  | 'invalid-response'
  | 'unknown-id'
  | 'duplicate-id';

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

export class ClientClosedError extends Error {
  constructor(message = 'TOON-RPC client is closed') {
    super(message);
    this.name = 'ClientClosedError';
  }
}

export class ClientAbortError extends Error {
  constructor() {
    super('TOON-RPC operation was aborted');
    this.name = 'ClientAbortError';
  }
}

export class ClientTimeoutError extends Error {
  constructor(timeoutMs: number) {
    super(`TOON-RPC call timed out after ${timeoutMs}ms`);
    this.name = 'ClientTimeoutError';
  }
}

export class ClientProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ClientProtocolError';
  }
}

interface PendingCall {
  resolve: (value: CoreValue) => void;
  reject: (error: Error) => void;
  signal?: AbortSignal;
  abort?: () => void;
  timer?: ReturnType<typeof setTimeout>;
  controller: AbortController;
}

interface ResponseScope {
  id?: Id;
}

const hasOwn = Object.hasOwn;

export class Client {
  private idCounter = 0;
  private readonly pending = new Map<Id, PendingCall>();
  private state: ClientStatus = 'idle';
  private terminalError: Error | undefined;
  private readonly terminalController = new AbortController();
  private resolveTermination!: (error: Error) => void;
  private readonly termination = new Promise<Error>((resolve) => {
    this.resolveTermination = resolve;
  });
  private startPromise: Promise<void> | undefined;
  private receivePromise: Promise<void> | undefined;
  private transportClosePromise: Promise<void> | undefined;
  private closePromise: Promise<void> | undefined;

  constructor(
    private readonly transport: ClientTransport,
    private readonly options: ClientOptions = {}
  ) {}

  get status(): ClientStatus {
    return this.state;
  }

  get pendingCallCount(): number {
    return this.pending.size;
  }

  start(): Promise<void> {
    return this.ensureOpen();
  }

  call(method: string, params?: Params, options: CallOptions = {}): Promise<CoreValue> {
    let id: Id;
    let document: Uint8Array;
    let timeoutMs: number | undefined;
    let signal: AbortSignal | undefined;
    try {
      signal = options.signal;
      if (signal?.aborted) return Promise.reject(new ClientAbortError());
      timeoutMs = validateTimeout(options.timeoutMs);
      id = hasOwn(options, 'id') ? options.id! : this.allocateId();
      if (!isId(id)) throw new TypeError('TOON-RPC call ID must be a string, safe integer, or null');
      if (this.pending.has(id)) throw new Error(`TOON-RPC call ID is already pending: ${String(id)}`);
      document = encodeRequest(method, params, id);
    } catch (error) {
      return Promise.reject(asError(error));
    }

    return new Promise<CoreValue>((resolve, reject) => {
      const pending: PendingCall = { resolve, reject, controller: new AbortController() };
      this.pending.set(id, pending);
      if (signal) {
        pending.signal = signal;
        pending.abort = () => this.rejectPending(id, new ClientAbortError());
        signal.addEventListener('abort', pending.abort, { once: true });
        if (signal.aborted) pending.abort();
      }
      if (timeoutMs !== undefined && this.pending.has(id)) {
        pending.timer = setTimeout(
          () => this.rejectPending(id, new ClientTimeoutError(timeoutMs)),
          timeoutMs
        );
      }
      if (this.pending.has(id)) void this.dispatchCall(id, document);
    });
  }

  async notify(method: string, params?: Params, options: NotifyOptions = {}): Promise<void> {
    const timeoutMs = validateTimeout(options.timeoutMs);
    if (options.signal?.aborted) throw new ClientAbortError();
    const document = encodeRequest(method, params);
    const operation = new OperationScope(
      options.signal,
      timeoutMs,
      this.terminalController.signal,
      () => this.terminalError ?? new ClientClosedError()
    );
    try {
      await operation.race(this.ensureOpen());
      this.assertOpen();
      if (this.transport.kind === 'duplex') {
        await operation.race(
          this.transport.send(document, operationOptions(operation.signal))
        );
      } else {
        const response = await operation.race(
          this.transport.request(document, operationOptions(operation.signal))
        );
        if (response && response.length > 0) this.processDocument(response, {});
      }
    } finally {
      operation.dispose();
    }
  }

  close(): Promise<void> {
    if (this.closePromise) return this.closePromise;
    if (this.state !== 'closed' && this.state !== 'failed') {
      this.terminate('closed', new ClientClosedError());
    }
    this.closePromise = (async () => {
      const [transportResult, receiveResult] = await Promise.allSettled([
        this.closeTransport(),
        this.receivePromise ?? Promise.resolve(),
      ]);
      if (transportResult.status === 'rejected') throw transportResult.reason;
      if (receiveResult.status === 'rejected') throw receiveResult.reason;
    })();
    return this.closePromise;
  }

  private async dispatchCall(id: Id, document: Uint8Array): Promise<void> {
    try {
      await this.ensureOpen();
      const pending = this.pending.get(id);
      if (!pending) return;
      this.assertOpen();
      if (this.transport.kind === 'duplex') {
        await this.transport.send(document, operationOptions(pending.controller.signal));
        return;
      }

      const response = await this.transport.request(
        document,
        operationOptions(pending.controller.signal)
      );
      if (!this.pending.has(id)) return;
      if (!response || response.length === 0) {
        this.rejectPending(id, new ClientProtocolError('Request/response transport returned no response'));
        return;
      }
      this.processDocument(response, { id });
      if (this.pending.has(id)) {
        this.rejectPending(
          id,
          new ClientProtocolError('Request/response document did not contain the matching response')
        );
      }
    } catch (error) {
      this.rejectPending(id, asError(error));
    }
  }

  private ensureOpen(): Promise<void> {
    if (this.state === 'open') return Promise.resolve();
    if (this.state === 'closed' || this.state === 'failed') {
      return Promise.reject(this.terminalError ?? new ClientClosedError());
    }
    if (this.startPromise) return this.startPromise;

    this.state = 'opening';
    this.startPromise = (async () => {
      try {
        const opening = this.transport.open?.({ signal: this.terminalController.signal }) ?? Promise.resolve();
        await Promise.race([
          opening,
          this.termination.then((error) => Promise.reject(error)),
        ]);
        if (this.state !== 'opening') throw this.terminalError ?? new ClientClosedError();
        this.state = 'open';
        if (this.transport.kind === 'duplex') {
          this.receivePromise = this.receiveLoop();
        }
      } catch (error) {
        const failure = asError(error);
        if (this.state !== 'closed' && this.state !== 'failed') this.terminate('failed', failure);
        throw failure;
      }
    })();
    return this.startPromise;
  }

  private async receiveLoop(): Promise<void> {
    if (this.transport.kind !== 'duplex') return;
    try {
      for await (const document of this.transport.receive({ signal: this.terminalController.signal })) {
        if (this.state !== 'open') return;
        this.processDocument(document);
      }
      if (this.state === 'open') {
        this.terminate('closed', new ClientClosedError('TOON-RPC transport closed'));
      }
    } catch (error) {
      if (this.state === 'open') this.terminate('failed', asError(error));
    }
  }

  private processDocument(document: Uint8Array, scope?: ResponseScope): void {
    let value: unknown;
    try {
      const text = new TextDecoder('utf-8', { fatal: true }).decode(document);
      value = decode(text);
    } catch (error) {
      this.diagnostic({ reason: 'parse-error', error });
      return;
    }

    if (!Array.isArray(value)) {
      const response = snapshotResponse(value);
      if (!response) {
        this.diagnostic({ reason: 'invalid-response' });
        return;
      }
      this.settleResponse(response, undefined, scope);
      return;
    }
    if (value.length === 0) {
      this.diagnostic({ reason: 'invalid-response' });
      return;
    }

    const settledIds = new Set<Id>();
    value.forEach((entry, index) => {
      const response = snapshotResponse(entry);
      if (!response) {
        this.diagnostic({ reason: 'invalid-response', index });
      } else if (settledIds.has(response.id)) {
        this.diagnostic({ reason: 'duplicate-id', id: response.id, index });
      } else if (this.settleResponse(response, index, scope)) {
        settledIds.add(response.id);
      }
    });
  }

  private settleResponse(response: Response, index?: number, scope?: ResponseScope): boolean {
    if (scope && (!hasOwn(scope, 'id') || response.id !== scope.id)) {
      this.diagnostic({ reason: 'unknown-id', id: response.id, ...(index === undefined ? {} : { index }) });
      return false;
    }
    const pending = this.takePending(response.id);
    if (!pending) {
      this.diagnostic({ reason: 'unknown-id', id: response.id, ...(index === undefined ? {} : { index }) });
      return false;
    }
    if ('error' in response) {
      const error = hasOwn(response.error, 'data')
        ? new RpcError(response.error.code, response.error.message, response.error.data!)
        : new RpcError(response.error.code, response.error.message);
      pending.reject(error);
    } else {
      pending.resolve(response.result);
    }
    return true;
  }

  private rejectPending(id: Id, error: Error): boolean {
    const pending = this.takePending(id);
    if (!pending) return false;
    pending.reject(error);
    return true;
  }

  private takePending(id: Id): PendingCall | undefined {
    const pending = this.pending.get(id);
    if (!pending) return undefined;
    this.pending.delete(id);
    if (pending.timer !== undefined) clearTimeout(pending.timer);
    if (pending.signal && pending.abort) pending.signal.removeEventListener('abort', pending.abort);
    pending.controller.abort();
    return pending;
  }

  private rejectAll(error: Error): void {
    for (const id of [...this.pending.keys()]) this.rejectPending(id, error);
  }

  private terminate(status: 'closed' | 'failed', error: Error): void {
    if (this.state === 'closed' || this.state === 'failed') return;
    this.state = status;
    this.terminalError = error;
    this.resolveTermination(error);
    this.terminalController.abort(error);
    this.rejectAll(error);
    void this.closeTransport().catch(() => {});
  }

  private closeTransport(): Promise<void> {
    this.transportClosePromise ??= Promise.resolve().then(() => this.transport.close());
    return this.transportClosePromise;
  }

  private assertOpen(): void {
    if (this.state !== 'open') throw this.terminalError ?? new ClientClosedError();
  }

  private diagnostic(diagnostic: ClientDiagnostic): void {
    try {
      this.options.onDiagnostic?.(diagnostic);
    } catch {
      // Diagnostics cannot take ownership of the receive loop.
    }
  }

  private allocateId(): number {
    while (this.pending.has(this.idCounter)) this.idCounter += 1;
    if (!Number.isSafeInteger(this.idCounter)) throw new Error('TOON-RPC numeric ID space exhausted');
    return this.idCounter++;
  }
}

function encodeRequest(method: string, params: Params | undefined, id?: Id): Uint8Array {
  const source = {
    toonrpc: TOONRPC_VERSION,
    method,
    ...(params === undefined ? {} : { params }),
    ...(arguments.length >= 3 ? { id } : {}),
  };
  const request = snapshotRequestObject(source);
  if (!request) throw new TypeError('Invalid TOON-RPC request');
  return new TextEncoder().encode(encode(request as unknown as JsonValue));
}

function validateTimeout(timeoutMs: number | undefined): number | undefined {
  if (timeoutMs === undefined) return undefined;
  if (!Number.isFinite(timeoutMs) || timeoutMs < 0 || timeoutMs > 2147483647) {
    throw new RangeError('TOON-RPC timeout must be between 0 and 2147483647ms');
  }
  return timeoutMs;
}

function operationOptions(signal: AbortSignal | undefined): TransportOperationOptions | undefined {
  return signal ? { signal } : undefined;
}

function asError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

class OperationScope {
  readonly controller = new AbortController();
  readonly signal = this.controller.signal;
  private readonly cancellation: Promise<never>;
  private rejectCancellation!: (error: Error) => void;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private active = true;

  constructor(
    private readonly callerSignal: AbortSignal | undefined,
    timeoutMs: number | undefined,
    private readonly terminalSignal: AbortSignal,
    private readonly terminalError: () => Error
  ) {
    this.cancellation = new Promise((_, reject) => {
      this.rejectCancellation = reject;
    });
    callerSignal?.addEventListener('abort', this.abortFromCaller, { once: true });
    terminalSignal.addEventListener('abort', this.abortFromTerminal, { once: true });
    if (timeoutMs !== undefined) {
      this.timer = setTimeout(() => this.cancel(new ClientTimeoutError(timeoutMs)), timeoutMs);
    }
    if (callerSignal?.aborted) this.abortFromCaller();
    else if (terminalSignal.aborted) this.abortFromTerminal();
  }

  race<T>(operation: Promise<T>): Promise<T> {
    return Promise.race([operation, this.cancellation]);
  }

  dispose(): void {
    if (!this.active) return;
    this.active = false;
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.callerSignal?.removeEventListener('abort', this.abortFromCaller);
    this.terminalSignal.removeEventListener('abort', this.abortFromTerminal);
  }

  private readonly abortFromCaller = () => this.cancel(new ClientAbortError());
  private readonly abortFromTerminal = () => this.cancel(this.terminalError());

  private cancel(error: Error): void {
    if (!this.active) return;
    this.dispose();
    this.controller.abort(error);
    this.rejectCancellation(error);
  }
}
