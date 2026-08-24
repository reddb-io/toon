/**
 * TCP transport for TOON-RPC (Node.js).
 *
 * TCP is a byte stream with no document boundaries of its own, so this
 * transport speaks the length-prefixed stream framing profile from
 * `framing.ts` — never newline inference. Each sent document becomes one
 * frame; received bytes are reassembled into complete documents across
 * arbitrary chunk splits.
 */

import * as net from 'node:net';
import type { DuplexTransport, TransportOperationOptions } from './transport.js';
import { FrameDecoder, encodeFrame } from './framing.js';
import { DocumentQueue, abortError, asTransportError, raceSignal } from './internal.js';

export interface TcpTransportOptions {
  host?: string;
  port?: number;
  /** Injectable socket factory; defaults to net.createConnection(port, host). */
  connect?: () => net.Socket;
}

export class TcpTransport implements DuplexTransport {
  readonly kind = 'duplex' as const;
  private readonly options: TcpTransportOptions;
  private readonly documents = new DocumentQueue();
  private readonly decoder = new FrameDecoder();
  private socket: net.Socket | undefined;
  private openPromise: Promise<void> | undefined;
  private closePromise: Promise<void> | undefined;
  private failure: Error | undefined;

  constructor(options: TcpTransportOptions) {
    if (!options.connect && (options.host === undefined || options.port === undefined)) {
      throw new TypeError('TcpTransport needs host and port, or a connect factory');
    }
    this.options = options;
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
    if (!socket || socket.destroyed || socket.writableEnded) {
      throw new Error('TOON-RPC TCP transport is not open');
    }
    await new Promise<void>((resolve, reject) => {
      socket.write(encodeFrame(document), (error) => {
        if (error) reject(asTransportError(error));
        else resolve();
      });
    });
  }

  receive(options?: TransportOperationOptions): AsyncIterable<Uint8Array> {
    return this.documents.iterate(options);
  }

  close(): Promise<void> {
    this.closePromise ??= new Promise<void>((resolve) => {
      this.documents.end();
      const socket = this.socket;
      if (!socket || socket.destroyed) {
        resolve();
        return;
      }
      socket.once('close', () => resolve());
      socket.destroy();
    });
    return this.closePromise;
  }

  private connect(): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      let socket: net.Socket;
      try {
        socket = this.options.connect
          ? this.options.connect()
          : net.createConnection({ host: this.options.host!, port: this.options.port! });
      } catch (error) {
        reject(asTransportError(error));
        return;
      }
      this.socket = socket;

      let settled = false;
      const onConnect = () => {
        settled = true;
        resolve();
      };
      if (this.options.connect && !socket.connecting) onConnect();
      else socket.once('connect', onConnect);

      socket.on('data', (chunk: Buffer) => {
        try {
          for (const document of this.decoder.push(new Uint8Array(chunk))) {
            this.documents.push(document);
          }
        } catch (error) {
          this.failWith(asTransportError(error));
          socket.destroy();
        }
      });
      socket.on('error', (error: Error) => {
        const failure = asTransportError(error);
        if (!settled) {
          settled = true;
          reject(failure);
        }
        this.failWith(failure);
      });
      socket.on('close', () => {
        if (!settled) {
          settled = true;
          reject(this.failure ?? new Error('TCP connection closed before opening'));
          return;
        }
        try {
          if (!this.failure) this.decoder.finish();
          this.documents.end();
        } catch (error) {
          this.failWith(asTransportError(error));
        }
      });
    });
  }

  private failWith(error: Error): void {
    this.failure ??= error;
    this.documents.fail(error);
  }
}

export function createTcpTransport(options: TcpTransportOptions): TcpTransport {
  return new TcpTransport(options);
}
