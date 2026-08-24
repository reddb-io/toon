/**
 * Server-Sent Events transport for TOON-RPC.
 *
 * SSE composes a duplex profile out of two HTTP legs: documents travel to the
 * server as POST bodies, and documents travel back as complete `data:` events
 * on one long-lived event stream. A multi-line TOON document arrives as one
 * event whose data lines rejoin with LF exactly as the SSE specification
 * defines, so document boundaries are the event boundaries.
 *
 * Built on fetch streaming rather than EventSource: EventSource cannot POST,
 * cannot send headers, and cannot abort deterministically.
 */

import type { DuplexTransport, TransportOperationOptions } from './transport.js';
import { TOON_RPC_CONTENT_TYPE } from './http.js';
import { DocumentQueue, abortError, asTransportError, raceSignal } from './internal.js';

export interface SseTransportOptions {
  /** The event-stream URL documents are received from. */
  url: string | URL;
  /** The URL documents are POSTed to; defaults to `url`. */
  postUrl?: string | URL;
  headers?: Record<string, string>;
  /** Injectable fetch implementation; defaults to the global fetch. */
  fetch?: typeof fetch;
}

export class SseTransportError extends Error {
  constructor(
    public readonly status: number,
    statusText: string
  ) {
    super(`TOON-RPC SSE request failed: ${status} ${statusText}`.trimEnd());
    this.name = 'SseTransportError';
  }
}

export class SseTransport implements DuplexTransport {
  readonly kind = 'duplex' as const;
  private readonly url: string;
  private readonly postUrl: string;
  private readonly headers: Record<string, string>;
  private readonly fetchImpl: typeof fetch;
  private readonly documents = new DocumentQueue();
  private readonly lifetime = new AbortController();
  private openPromise: Promise<void> | undefined;
  private pumpPromise: Promise<void> | undefined;
  private closed = false;
  private failure: Error | undefined;

  constructor(options: SseTransportOptions) {
    this.url = String(options.url);
    this.postUrl = String(options.postUrl ?? options.url);
    this.headers = { ...options.headers };
    this.fetchImpl = options.fetch ?? fetch;
  }

  open(options?: TransportOperationOptions): Promise<void> {
    this.openPromise ??= raceSignal(this.connect(), options?.signal);
    return this.openPromise;
  }

  async send(document: Uint8Array, options?: TransportOperationOptions): Promise<void> {
    await this.open(options);
    if (options?.signal?.aborted || this.lifetime.signal.aborted) throw abortError();
    if (this.failure) throw this.failure;
    const response = await this.fetchImpl(this.postUrl, {
      method: 'POST',
      body: document as unknown as BodyInit,
      headers: { 'Content-Type': TOON_RPC_CONTENT_TYPE, ...this.headers },
      signal: options?.signal ?? this.lifetime.signal,
    });
    await response.arrayBuffer().catch(() => undefined);
    if (!response.ok) {
      throw new SseTransportError(response.status, response.statusText ?? '');
    }
  }

  receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array> {
    return this.documents.iterate(options);
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    this.documents.end();
    this.lifetime.abort(abortError());
    await this.pumpPromise?.catch(() => undefined);
  }

  private async connect(): Promise<void> {
    const response = await this.fetchImpl(this.url, {
      method: 'GET',
      headers: { Accept: 'text/event-stream', ...this.headers },
      signal: this.lifetime.signal,
    });
    if (!response.ok) {
      await response.arrayBuffer().catch(() => undefined);
      throw new SseTransportError(response.status, response.statusText ?? '');
    }
    if (!response.body) throw new Error('TOON-RPC SSE stream has no body');
    this.pumpPromise = this.pump(response.body);
  }

  private async pump(body: ReadableStream<Uint8Array>): Promise<void> {
    const reader = body.getReader();
    const parser = new SseEventParser((data) => {
      this.documents.push(new TextEncoder().encode(data));
    });
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value) parser.push(value);
      }
      this.documents.end();
    } catch (error) {
      if (this.closed || this.lifetime.signal.aborted) {
        this.documents.end();
        return;
      }
      this.failure ??= asTransportError(error);
      this.documents.fail(this.failure);
    }
  }
}

export function createSseTransport(options: SseTransportOptions): SseTransport {
  return new SseTransport(options);
}

/** Minimal SSE parser: only complete events with data dispatch a document. */
class SseEventParser {
  private readonly decoder = new TextDecoder('utf-8');
  private buffer = '';
  private dataLines: string[] = [];
  private hasData = false;

  constructor(private readonly onEvent: (data: string) => void) {}

  push(chunk: Uint8Array): void {
    this.buffer += this.decoder.decode(chunk, { stream: true });
    for (;;) {
      const lineEnd = this.buffer.indexOf('\n');
      if (lineEnd === -1) return;
      let line = this.buffer.slice(0, lineEnd);
      this.buffer = this.buffer.slice(lineEnd + 1);
      if (line.endsWith('\r')) line = line.slice(0, -1);
      this.processLine(line);
    }
  }

  private processLine(line: string): void {
    if (line === '') {
      if (this.hasData) this.onEvent(this.dataLines.join('\n'));
      this.dataLines = [];
      this.hasData = false;
      return;
    }
    if (line.startsWith(':')) return;
    const colon = line.indexOf(':');
    const field = colon === -1 ? line : line.slice(0, colon);
    if (field !== 'data') return;
    let value = colon === -1 ? '' : line.slice(colon + 1);
    if (value.startsWith(' ')) value = value.slice(1);
    this.dataLines.push(value);
    this.hasData = true;
  }
}
