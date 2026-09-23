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
/** Serve TOON-RPC over TCP with §8.1 framing. Binds 127.0.0.1:0 by default. */
export declare function serveTcp(handler: DocumentHandler, options?: ServeOptions & {
    host?: string;
    port?: number;
}): Promise<TcpServerHandle>;
/** Serve TOON-RPC over this process's stdin and stdout with §8.1 framing. */
export declare function serveStdio(handler: DocumentHandler, options?: ServeOptions & {
    input?: Readable;
    output?: Writable;
}): ServerHandle & {
    readonly done: Promise<void>;
};
type NodeHandler = (request: IncomingMessage, response: ServerResponse) => void;
/**
 * A `node:http` request listener answering one POST per document: 200 with
 * a TOON body, 204 when there is no response, 405 for other methods and 413
 * past the body limit.
 */
export declare function createHttpHandler(handler: DocumentHandler, options?: ServeOptions): NodeHandler;
export interface SseHandler extends NodeHandler {
    /** Sessions with an open event stream. */
    readonly sessionCount: number;
    /** End every open event stream. */
    closeSessions(): void;
}
/** Encode one document as one SSE event: each line becomes a `data:` line. */
export declare function encodeEvent(document: Uint8Array): string;
/**
 * A `node:http` request listener for the §8.2 SSE profile: `GET ?session=ID`
 * opens the event stream, `POST ?session=ID` is acknowledged with 202 and
 * its response arrives on that stream. 400 without a session, 409 for a
 * second stream, 404 for a POST to an unknown session, 410 when the stream
 * closed while the POST was being answered.
 */
export declare function createSseHandler(handler: DocumentHandler, options?: ServeOptions): SseHandler;
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
export declare function attachWebSocket(handler: DocumentHandler, socket: ServerWebSocket, options?: ServeOptions): ServerHandle;
export {};
//# sourceMappingURL=serve.d.ts.map