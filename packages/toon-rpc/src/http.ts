/**
 * HTTP transport for TOON-RPC.
 *
 * HTTP is a request/response exchange, not a duplex stream: each POST carries
 * one complete RPC document and directly owns zero or one response document.
 * A notification-only exchange comes back with status 204 (or an empty body)
 * and produces no response document.
 */

import type { RequestResponseTransport, TransportOperationOptions } from './transport.js';
import { abortError } from './internal.js';

export const TOON_RPC_CONTENT_TYPE = 'application/toon';

export interface HttpTransportOptions {
  url: string | URL;
  headers?: Record<string, string>;
  /** Injectable fetch implementation; defaults to the global fetch. */
  fetch?: typeof fetch;
}

export class HttpTransportError extends Error {
  constructor(
    public readonly status: number,
    statusText: string
  ) {
    super(`TOON-RPC HTTP request failed: ${status} ${statusText}`.trimEnd());
    this.name = 'HttpTransportError';
  }
}

export class HttpTransport implements RequestResponseTransport {
  readonly kind = 'request-response' as const;
  private readonly url: string;
  private readonly headers: Record<string, string>;
  private readonly fetchImpl: typeof fetch;
  private readonly lifetime = new AbortController();
  private closed = false;

  constructor(options: HttpTransportOptions) {
    this.url = String(options.url);
    this.headers = { ...options.headers };
    this.fetchImpl = options.fetch ?? fetch;
  }

  async request(
    document: Uint8Array,
    options?: TransportOperationOptions
  ): Promise<Uint8Array | undefined> {
    if (this.closed) throw new Error('TOON-RPC HTTP transport is closed');
    if (options?.signal?.aborted || this.lifetime.signal.aborted) throw abortError();

    const signal = mergeSignals(this.lifetime.signal, options?.signal);
    const response = await this.fetchImpl(this.url, {
      method: 'POST',
      body: document as unknown as BodyInit,
      headers: {
        'Content-Type': TOON_RPC_CONTENT_TYPE,
        Accept: TOON_RPC_CONTENT_TYPE,
        ...this.headers,
      },
      signal,
    });

    if (!response.ok) {
      await response.arrayBuffer().catch(() => undefined);
      throw new HttpTransportError(response.status, response.statusText ?? '');
    }
    if (response.status === 204) return undefined;

    const body = new Uint8Array(await response.arrayBuffer());
    return body.length === 0 ? undefined : body;
  }

  async close(): Promise<void> {
    this.closed = true;
    this.lifetime.abort(abortError());
  }
}

export function createHttpTransport(options: HttpTransportOptions): HttpTransport {
  return new HttpTransport(options);
}

function mergeSignals(lifetime: AbortSignal, operation?: AbortSignal): AbortSignal {
  if (!operation) return lifetime;
  if (typeof AbortSignal.any === 'function') return AbortSignal.any([lifetime, operation]);
  const controller = new AbortController();
  const forward = () => controller.abort(abortError());
  lifetime.addEventListener('abort', forward, { once: true });
  operation.addEventListener('abort', forward, { once: true });
  return controller.signal;
}
