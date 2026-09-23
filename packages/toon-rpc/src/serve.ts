/**
 * Node.js servers for every TOON-RPC transport.
 *
 * Each one feeds complete documents to a document handler (a `Server`, or
 * anything with the same `handle` method) and sends back its non-empty
 * answers, under the shared limits from `limits.ts`:
 *
 * - `serveTcp` and `serveStdio` speak the §8.1 length-prefixed framing;
 * - `createHttpHandler` answers one POST per document (204 for no response);
 * - `createSseHandler` is the §8.2 duplex profile, bound by a `session`
 *   query parameter on the GET and every POST;
 * - `attachWebSocket` serves one socket shaped like the `ws` package's.
 *
 * Documents from one connection are answered one at a time, in order. A
 * framing error, an oversized frame, an idle connection or an overfull
 * queue closes that connection. Closing a server stops accepting, lets each
 * connection answer the documents it already received, and ends it;
 * whatever is still running after the shutdown grace period is destroyed.
 */

import * as net from 'node:net';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Readable, Writable } from 'node:stream';
import { FrameDecoder, encodeFrame } from './framing.js';
import { TOON_RPC_CONTENT_TYPE } from './http.js';
import { resolveLimits } from './limits.js';
import type { Limits } from './limits.js';

/** Anything that answers one RPC document; `Server` and `MultiRpc` both do. */
export interface DocumentHandler {
  handle(document: Uint8Array): Promise<Uint8Array>;
}

export interface ServeOptions {
  limits?: Partial<Limits>;
}

/** A running server. `close` shuts it down gracefully. */
export interface ServerHandle {
  close(): Promise<void>;
}

export interface TcpServerHandle extends ServerHandle {
  readonly address: net.AddressInfo;
}

/**
 * Answers the documents of one connection in order, with at most
 * `maxQueuedDocuments` waiting; `idle` resolves when nothing is in flight.
 */
class DocumentLane {
  private tail: Promise<void> = Promise.resolve();
  private queued = 0;

  constructor(
    private readonly handler: DocumentHandler,
    private readonly maxQueued: number,
    private readonly fail: () => void
  ) {}

  /**
   * Queue `document`; `reply` sends its non-empty answer. False when the
   * queue is full, and the caller must close the connection.
   */
  accept(document: Uint8Array, reply: (answer: Uint8Array) => Promise<void> | void): boolean {
    if (this.queued >= this.maxQueued) return false;
    this.queued += 1;
    this.tail = this.tail.then(async () => {
      try {
        const answer = await this.handler.handle(document);
        if (answer.length > 0) await reply(answer);
      } catch {
        this.fail();
      } finally {
        this.queued -= 1;
      }
    });
    return true;
  }

  idle(): Promise<void> {
    return this.tail;
  }
}

/** Resolve after `ms` without keeping the process alive. */
function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms).unref?.());
}

/** Serve one framed byte stream; returns its graceful close. */
function serveFramedStream(
  handler: DocumentHandler,
  input: Readable,
  output: Writable,
  limits: Limits,
  destroy: () => void
): () => Promise<void> {
  const decoder = new FrameDecoder({ maxFrameBytes: limits.maxFrameBytes });
  let closing = false;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  const armIdle = () => {
    if (idleTimer) clearTimeout(idleTimer);
    if (limits.idleTimeoutMs !== undefined) idleTimer = setTimeout(destroy, limits.idleTimeoutMs);
  };
  const lane = new DocumentLane(handler, limits.maxQueuedDocuments, destroy);
  const reply = (answer: Uint8Array) =>
    new Promise<void>((resolve, reject) => {
      output.write(encodeFrame(answer), (error) => (error ? reject(error) : resolve()));
    });

  armIdle();
  input.on('data', (chunk: Buffer | string) => {
    if (closing) return;
    armIdle();
    const bytes = typeof chunk === 'string' ? new TextEncoder().encode(chunk) : new Uint8Array(chunk);
    let documents: Uint8Array[];
    try {
      documents = decoder.push(bytes);
    } catch {
      destroy();
      return;
    }
    for (const document of documents) {
      if (!lane.accept(document, reply)) {
        destroy();
        return;
      }
    }
  });
  let finished: Promise<void> | undefined;
  // Ends the output once every received document is answered, and resolves
  // only after the answers are flushed, so a later destroy cannot cut them.
  const finish = () =>
    (finished ??= (async () => {
      if (idleTimer) clearTimeout(idleTimer);
      await lane.idle();
      if (output.writableEnded) return;
      await new Promise<void>((resolve) => output.end(resolve));
    })());
  input.on('end', () => void finish());
  input.on('error', destroy);
  input.on('close', () => {
    if (idleTimer) clearTimeout(idleTimer);
  });

  return async () => {
    closing = true;
    input.pause();
    await finish();
  };
}

/** Close every connection gracefully within the grace period, then destroy. */
async function drain(
  closers: Iterable<{ close: () => Promise<void>; destroy: () => void }>,
  graceMs: number
): Promise<void> {
  const all = [...closers];
  const closed = Promise.allSettled(all.map(({ close }) => close()));
  await Promise.race([closed, delay(graceMs)]);
  for (const { destroy } of all) destroy();
}

/** Serve TOON-RPC over TCP with §8.1 framing. Binds 127.0.0.1:0 by default. */
export async function serveTcp(
  handler: DocumentHandler,
  options: ServeOptions & { host?: string; port?: number } = {}
): Promise<TcpServerHandle> {
  const limits = resolveLimits(options.limits);
  const connections = new Set<{ close: () => Promise<void>; destroy: () => void }>();
  const tcp = net.createServer((socket) => {
    const destroy = () => socket.destroy();
    const connection = { close: serveFramedStream(handler, socket, socket, limits, destroy), destroy };
    connections.add(connection);
    socket.on('close', () => connections.delete(connection));
  });
  tcp.maxConnections = limits.maxConnections;
  await new Promise<void>((resolve, reject) => {
    tcp.once('error', reject);
    tcp.listen(options.port ?? 0, options.host ?? '127.0.0.1', () => {
      tcp.off('error', reject);
      resolve();
    });
  });
  const address = tcp.address() as net.AddressInfo;
  return {
    address,
    async close() {
      const stopped = new Promise<void>((resolve) => tcp.close(() => resolve()));
      await drain(connections, limits.shutdownGraceMs);
      await stopped;
    },
  };
}

/** Serve TOON-RPC over this process's stdin and stdout with §8.1 framing. */
export function serveStdio(
  handler: DocumentHandler,
  options: ServeOptions & { input?: Readable; output?: Writable } = {}
): ServerHandle & { readonly done: Promise<void> } {
  const limits = { ...resolveLimits(options.limits), idleTimeoutMs: undefined };
  const input = options.input ?? process.stdin;
  const output = options.output ?? process.stdout;
  const done = new Promise<void>((resolve) => {
    output.on('finish', resolve);
    output.on('close', resolve);
  });
  const close = serveFramedStream(handler, input, output, limits, () => {
    input.destroy();
    output.destroy();
  });
  return {
    done,
    async close() {
      await Promise.race([close(), delay(limits.shutdownGraceMs)]);
    },
  };
}

type NodeHandler = (request: IncomingMessage, response: ServerResponse) => void;

/** Read a request body, or answer 413 and return undefined past the limit. */
function readBody(
  request: IncomingMessage,
  response: ServerResponse,
  maxBodyBytes: number
): Promise<Uint8Array | undefined> {
  const declared = Number(request.headers['content-length']);
  if (declared > maxBodyBytes) {
    response.writeHead(413).end();
    request.resume();
    return Promise.resolve(undefined);
  }
  return new Promise((resolve) => {
    const chunks: Buffer[] = [];
    let total = 0;
    let refused = false;
    request.on('data', (chunk: Buffer) => {
      if (refused) return;
      total += chunk.length;
      if (total > maxBodyBytes) {
        refused = true;
        response.writeHead(413).end();
        resolve(undefined);
        return;
      }
      chunks.push(chunk);
    });
    request.on('end', () => {
      if (!refused) resolve(new Uint8Array(Buffer.concat(chunks)));
    });
    request.on('error', () => {
      if (!refused) {
        refused = true;
        if (!response.headersSent) response.writeHead(400).end();
        resolve(undefined);
      }
    });
  });
}

/**
 * A `node:http` request listener answering one POST per document: 200 with
 * a TOON body, 204 when there is no response, 405 for other methods and 413
 * past the body limit.
 */
export function createHttpHandler(handler: DocumentHandler, options: ServeOptions = {}): NodeHandler {
  const limits = resolveLimits(options.limits);
  return (request, response) => {
    if (request.method !== 'POST') {
      response.writeHead(405, { Allow: 'POST' }).end();
      request.resume();
      return;
    }
    void readBody(request, response, limits.maxBodyBytes).then(async (body) => {
      if (!body) return;
      const answer = await handler.handle(body);
      if (answer.length === 0) {
        response.writeHead(204).end();
        return;
      }
      response.writeHead(200, { 'Content-Type': TOON_RPC_CONTENT_TYPE }).end(answer);
    });
  };
}

export interface SseHandler extends NodeHandler {
  /** Sessions with an open event stream. */
  readonly sessionCount: number;
  /** End every open event stream. */
  closeSessions(): void;
}

/** Encode one document as one SSE event: each line becomes a `data:` line. */
export function encodeEvent(document: Uint8Array): string {
  const text = new TextDecoder('utf-8').decode(document);
  return `${text
    .split('\n')
    .map((line) => `data: ${line}\n`)
    .join('')}\n`;
}

/**
 * A `node:http` request listener for the §8.2 SSE profile: `GET ?session=ID`
 * opens the event stream, `POST ?session=ID` is acknowledged with 202 and
 * its response arrives on that stream. 400 without a session, 409 for a
 * second stream, 404 for a POST to an unknown session, 410 when the stream
 * closed while the POST was being answered.
 */
export function createSseHandler(handler: DocumentHandler, options: ServeOptions = {}): SseHandler {
  const limits = resolveLimits(options.limits);
  const sessions = new Map<string, ServerResponse>();

  const listener: NodeHandler = (request, response) => {
    const session = new URL(request.url ?? '/', 'http://localhost').searchParams.get('session');
    if (!session) {
      response.writeHead(400).end();
      request.resume();
      return;
    }
    if (request.method === 'GET') {
      if (sessions.has(session)) {
        response.writeHead(409).end();
        return;
      }
      sessions.set(session, response);
      response.on('close', () => {
        if (sessions.get(session) === response) sessions.delete(session);
      });
      response.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' });
      response.write(': open\n\n');
      return;
    }
    if (request.method !== 'POST') {
      response.writeHead(405, { Allow: 'GET, POST' }).end();
      request.resume();
      return;
    }
    if (!sessions.has(session)) {
      response.writeHead(404).end();
      request.resume();
      return;
    }
    void readBody(request, response, limits.maxBodyBytes).then(async (body) => {
      if (!body) return;
      const answer = await handler.handle(body);
      if (answer.length > 0) {
        const stream = sessions.get(session);
        if (!stream) {
          response.writeHead(410).end();
          return;
        }
        // Waiting for the stream to drain is the backpressure.
        if (!stream.write(encodeEvent(answer))) {
          await new Promise<void>((resolve) => {
            stream.once('drain', resolve);
            stream.once('close', resolve);
          });
        }
      }
      response.writeHead(202).end();
    });
  };

  // defineProperties, not Object.assign: assign would copy the getter's
  // value once instead of the getter.
  return Object.defineProperties(listener, {
    sessionCount: { get: () => sessions.size },
    closeSessions: {
      value: () => {
        for (const stream of sessions.values()) stream.end();
        sessions.clear();
      },
    },
  }) as SseHandler;
}

/** The parts of a `ws` package WebSocket the server uses. */
export interface ServerWebSocket {
  on(event: 'message', listener: (data: unknown, isBinary: boolean) => void): unknown;
  on(event: 'close', listener: () => void): unknown;
  send(data: Uint8Array | string): void;
  close(code?: number, reason?: string): void;
}

/**
 * Serve one WebSocket connection: each message is one document, answered in
 * the kind of message (text or binary) it arrived as; a notification sends
 * nothing back. Past `maxFrameBytes` the socket closes with 1009, past the
 * idle timeout with 1001. Set the `ws` server's `maxPayload` to the same
 * limit so oversized messages are refused before they are buffered.
 */
export function attachWebSocket(
  handler: DocumentHandler,
  socket: ServerWebSocket,
  options: ServeOptions = {}
): ServerHandle {
  const limits = resolveLimits(options.limits);
  let closing = false;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  const armIdle = () => {
    if (idleTimer) clearTimeout(idleTimer);
    if (limits.idleTimeoutMs !== undefined) {
      idleTimer = setTimeout(() => socket.close(1001, 'idle timeout'), limits.idleTimeoutMs);
    }
  };
  const lane = new DocumentLane(handler, limits.maxQueuedDocuments, () =>
    socket.close(1011, 'server error')
  );

  armIdle();
  socket.on('message', (data, isBinary) => {
    if (closing) return;
    armIdle();
    const document = toBytes(data);
    if (document.length > limits.maxFrameBytes) {
      socket.close(1009, 'message too big');
      return;
    }
    const reply = (answer: Uint8Array) =>
      socket.send(isBinary ? answer : new TextDecoder().decode(answer));
    if (!lane.accept(document, reply)) socket.close(1008, 'too many queued messages');
  });
  socket.on('close', () => {
    closing = true;
    if (idleTimer) clearTimeout(idleTimer);
  });

  return {
    async close() {
      closing = true;
      if (idleTimer) clearTimeout(idleTimer);
      await Promise.race([lane.idle(), delay(limits.shutdownGraceMs)]);
      socket.close(1001, 'server shutting down');
    },
  };
}

function toBytes(data: unknown): Uint8Array {
  if (typeof data === 'string') return new TextEncoder().encode(data);
  if (Array.isArray(data)) return new Uint8Array(Buffer.concat(data as Buffer[]));
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  return new Uint8Array(0);
}
