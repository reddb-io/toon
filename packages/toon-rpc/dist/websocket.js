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
import { DocumentQueue, abortError, asTransportError, raceSignal } from './internal.js';
const WS_OPEN = 1;
export class WebSocketTransport {
    kind = 'duplex';
    url;
    implementation;
    documents = new DocumentQueue();
    socket;
    openPromise;
    closePromise;
    resolveClosed;
    closed = new Promise((resolve) => {
        this.resolveClosed = resolve;
    });
    failure;
    constructor(options) {
        this.url = String(options.url);
        const implementation = options.webSocket ?? globalThis.WebSocket;
        if (!implementation) {
            throw new Error('No WebSocket implementation available; pass options.webSocket');
        }
        this.implementation = implementation;
    }
    open(options) {
        this.openPromise ??= raceSignal(this.connect(), options?.signal);
        return this.openPromise;
    }
    async send(document, options) {
        await this.open(options);
        if (options?.signal?.aborted)
            throw abortError();
        if (this.failure)
            throw this.failure;
        const socket = this.socket;
        if (!socket || socket.readyState !== WS_OPEN) {
            throw new Error('TOON-RPC WebSocket transport is not open');
        }
        socket.send(document);
    }
    receive(options) {
        return this.documents.iterate(options);
    }
    close() {
        this.closePromise ??= (async () => {
            const socket = this.socket;
            this.documents.end();
            if (!socket) {
                this.resolveClosed?.();
                return;
            }
            try {
                socket.close(1000, 'client closed');
            }
            catch {
                this.resolveClosed?.();
                return;
            }
            await this.closed;
        })();
        return this.closePromise;
    }
    connect() {
        return new Promise((resolve, reject) => {
            let socket;
            try {
                socket = new this.implementation(this.url);
            }
            catch (error) {
                reject(asTransportError(error));
                return;
            }
            this.socket = socket;
            socket.binaryType = 'arraybuffer';
            let settled = false;
            socket.addEventListener('open', () => {
                settled = true;
                resolve();
            }, { once: true });
            socket.addEventListener('error', (event) => {
                const error = asTransportError(event?.error ?? new Error(event?.message ?? 'WebSocket error'));
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
            socket.addEventListener('message', (event) => {
                const document = normalizeFramePayload(event.data);
                if (document instanceof Uint8Array) {
                    this.documents.push(document);
                    return;
                }
                this.failWith(document);
                try {
                    socket.close(1003, 'unsupported frame payload');
                }
                catch {
                    // The failure is already recorded; closing is best-effort.
                }
            });
        });
    }
    failWith(error) {
        this.failure ??= error;
        this.documents.fail(error);
    }
}
export function createWebSocketTransport(options) {
    return new WebSocketTransport(options);
}
/** Node convenience: a transport backed by the `ws` package's WebSocket. */
export async function createNodeWebSocketTransport(url) {
    const module = (await import('ws'));
    const implementation = module.WebSocket ?? module.default;
    if (!implementation)
        throw new Error("The 'ws' package did not provide a WebSocket export");
    return new WebSocketTransport({ url, webSocket: implementation });
}
function normalizeFramePayload(data) {
    if (typeof data === 'string')
        return new TextEncoder().encode(data);
    if (data instanceof ArrayBuffer)
        return new Uint8Array(data.slice(0));
    if (ArrayBuffer.isView(data)) {
        return new Uint8Array(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
    }
    return new Error(`TOON-RPC WebSocket transport received an unsupported frame payload: ${describePayload(data)}`);
}
function describePayload(data) {
    if (data === null)
        return 'null';
    if (typeof data !== 'object')
        return typeof data;
    return data.constructor?.name ?? 'object';
}
//# sourceMappingURL=websocket.js.map