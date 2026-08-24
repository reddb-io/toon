/**
 * WebSocket transport for TOON-RPC.
 *
 * WebSocket is a framed duplex transport: every frame carries exactly one
 * complete RPC document. Text frames are decoded as UTF-8 documents; binary
 * frames are documents as-is; any other payload kind is a deterministic
 * transport failure, never a silent drop.
 *
 * The same class drives a browser `WebSocket` and the `ws` package's
 * `WebSocket` — both expose the addEventListener surface this transport
 * codes against. Pass the implementation via `options.webSocket` where the
 * global is absent (Node below 22, or to inject a test double).
 */

import type { DuplexTransport, TransportOperationOptions } from './transport.js';
import { DocumentQueue, abortError, asTransportError, raceSignal } from './internal.js';

interface WebSocketLike {
  readonly readyState: number;
  binaryType: string;
  send(data: Uint8Array): void;
  close(code?: number, reason?: string): void;
  addEventListener(type: string, listener: (event: never) => void, options?: { once?: boolean }): void;
}

interface WebSocketConstructor {
  new (url: string): WebSocketLike;
  readonly OPEN?: number;
}

export interface WebSocketTransportOptions {
  url: string | URL;
  /** WebSocket implementation; defaults to the global WebSocket. */
  webSocket?: WebSocketConstructor;
}

const WS_OPEN = 1;

export class WebSocketTransport implements DuplexTransport {
  readonly kind = 'duplex' as const;
  private readonly url: string;
  private readonly implementation: WebSocketConstructor;
  private readonly documents = new DocumentQueue();
  private socket: WebSocketLike | undefined;
  private openPromise: Promise<void> | undefined;
  private closePromise: Promise<void> | undefined;
  private resolveClosed: (() => void) | undefined;
  private readonly closed = new Promise<void>((resolve) => {
    this.resolveClosed = resolve;
  });
  private failure: Error | undefined;

  constructor(options: WebSocketTransportOptions) {
    this.url = String(options.url);
    const implementation =
      options.webSocket ?? (globalThis as { WebSocket?: WebSocketConstructor }).WebSocket;
    if (!implementation) {
      throw new Error('No WebSocket implementation available; pass options.webSocket');
    }
    this.implementation = implementation;
  }

  open(options?: TransportOperationOptions): Promise<void> {
    this.openPromise ??= raceSignal(this.connect(), options?.signal);
    return this.openPromise;
  }

  async send(document: Uint8Array, options?: TransportOperationOptions): Promise<void> {
    await this.open(options);
    if (options?.signal?.aborted) throw abortError();
    if (this.failure) throw this.failure;
    const socket = this.socket;
    if (!socket || socket.readyState !== WS_OPEN) {
      throw new Error('TOON-RPC WebSocket transport is not open');
    }
    socket.send(document);
  }

  receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array> {
    return this.documents.iterate(options);
  }

  close(): Promise<void> {
    this.closePromise ??= (async () => {
      const socket = this.socket;
      this.documents.end();
      if (!socket) {
        this.resolveClosed?.();
        return;
      }
      try {
        socket.close(1000, 'client closed');
      } catch {
        this.resolveClosed?.();
        return;
      }
      await this.closed;
    })();
    return this.closePromise;
  }

  private connect(): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      let socket: WebSocketLike;
      try {
        socket = new this.implementation(this.url);
      } catch (error) {
        reject(asTransportError(error));
        return;
      }
      this.socket = socket;
      socket.binaryType = 'arraybuffer';

      let settled = false;
      socket.addEventListener(
        'open',
        () => {
          settled = true;
          resolve();
        },
        { once: true }
      );
      socket.addEventListener('error', (event: { error?: unknown; message?: string }) => {
        const error = asTransportError(
          event?.error ?? new Error(event?.message ?? 'WebSocket error')
        );
        if (!settled) {
          settled = true;
          reject(error);
        }
        this.failWith(error);
      });
      socket.addEventListener('close', () => {
        if (!settled) {
          settled = true;
          reject(new Error('WebSocket closed before opening'));
        }
        this.documents.end();
        this.resolveClosed?.();
      });
      socket.addEventListener('message', (event: { data: unknown }) => {
        const document = normalizeFramePayload(event.data);
        if (document instanceof Uint8Array) {
          this.documents.push(document);
          return;
        }
        this.failWith(document);
        try {
          socket.close(1003, 'unsupported frame payload');
        } catch {
          // The failure is already recorded; closing is best-effort.
        }
      });
    });
  }

  private failWith(error: Error): void {
    this.failure ??= error;
    this.documents.fail(error);
  }
}

export function createWebSocketTransport(options: WebSocketTransportOptions): WebSocketTransport {
  return new WebSocketTransport(options);
}

/** Node convenience: a transport backed by the `ws` package's WebSocket. */
export async function createNodeWebSocketTransport(url: string | URL): Promise<WebSocketTransport> {
  const module = (await import('ws')) as unknown as {
    WebSocket?: WebSocketConstructor;
    default?: WebSocketConstructor;
  };
  const implementation = module.WebSocket ?? module.default;
  if (!implementation) throw new Error("The 'ws' package did not provide a WebSocket export");
  return new WebSocketTransport({ url, webSocket: implementation });
}

function normalizeFramePayload(data: unknown): Uint8Array | Error {
  if (typeof data === 'string') return new TextEncoder().encode(data);
  if (data instanceof ArrayBuffer) return new Uint8Array(data.slice(0));
  if (ArrayBuffer.isView(data)) {
    return new Uint8Array(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
  }
  return new Error(
    `TOON-RPC WebSocket transport received an unsupported frame payload: ${describePayload(data)}`
  );
}

function describePayload(data: unknown): string {
  if (data === null) return 'null';
  if (typeof data !== 'object') return typeof data;
  return data.constructor?.name ?? 'object';
}
