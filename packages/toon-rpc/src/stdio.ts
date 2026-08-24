/**
 * stdio transport for TOON-RPC (Node.js).
 *
 * A stdin/stdout pipe is a byte stream, so this transport speaks the
 * length-prefixed stream framing profile from `framing.ts` — one frame per
 * complete RPC document, reassembled across arbitrary chunk splits. The
 * streams are injectable for tests and for embedding a child process's
 * pipes; they default to this process's stdin/stdout.
 */

import type { Readable, Writable } from 'node:stream';
import type { DuplexTransport, TransportOperationOptions } from './transport.js';
import { FrameDecoder, encodeFrame } from './framing.js';
import { DocumentQueue, abortError, asTransportError } from './internal.js';

export interface StdioTransportOptions {
  input?: Readable;
  output?: Writable;
}

export class StdioTransport implements DuplexTransport {
  readonly kind = 'duplex' as const;
  private readonly input: Readable;
  private readonly output: Writable;
  private readonly documents = new DocumentQueue();
  private readonly decoder = new FrameDecoder();
  private started = false;
  private closed = false;
  private failure: Error | undefined;

  constructor(options: StdioTransportOptions = {}) {
    this.input = options.input ?? process.stdin;
    this.output = options.output ?? process.stdout;
  }

  async open(): Promise<void> {
    if (this.started) return;
    this.started = true;
    this.input.on('data', (chunk: Buffer | string) => {
      const bytes =
        typeof chunk === 'string' ? new TextEncoder().encode(chunk) : new Uint8Array(chunk);
      try {
        for (const document of this.decoder.push(bytes)) {
          this.documents.push(document);
        }
      } catch (error) {
        this.failWith(asTransportError(error));
      }
    });
    this.input.on('error', (error: Error) => this.failWith(asTransportError(error)));
    this.input.on('end', () => {
      try {
        if (!this.failure) this.decoder.finish();
        this.documents.end();
      } catch (error) {
        this.failWith(asTransportError(error));
      }
    });
  }

  async send(document: Uint8Array, options?: TransportOperationOptions): Promise<void> {
    await this.open();
    if (options?.signal?.aborted) throw abortError();
    if (this.failure) throw this.failure;
    if (this.closed) throw new Error('TOON-RPC stdio transport is closed');
    await new Promise<void>((resolve, reject) => {
      this.output.write(encodeFrame(document), (error) => {
        if (error) reject(asTransportError(error));
        else resolve();
      });
    });
  }

  receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array> {
    void this.open();
    return this.documents.iterate(options);
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    this.documents.end();
    this.input.pause?.();
  }

  private failWith(error: Error): void {
    this.failure ??= error;
    this.documents.fail(error);
  }
}

export function createStdioTransport(options: StdioTransportOptions = {}): StdioTransport {
  return new StdioTransport(options);
}
