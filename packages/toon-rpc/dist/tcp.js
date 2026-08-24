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
import { FrameDecoder, encodeFrame } from './framing.js';
import { DocumentQueue, abortError, asTransportError, raceSignal } from './internal.js';
export class TcpTransport {
    kind = 'duplex';
    options;
    documents = new DocumentQueue();
    decoder = new FrameDecoder();
    socket;
    openPromise;
    closePromise;
    failure;
    constructor(options) {
        if (!options.connect && (options.host === undefined || options.port === undefined)) {
            throw new TypeError('TcpTransport needs host and port, or a connect factory');
        }
        this.options = options;
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
        if (!socket || socket.destroyed || socket.writableEnded) {
            throw new Error('TOON-RPC TCP transport is not open');
        }
        await new Promise((resolve, reject) => {
            socket.write(encodeFrame(document), (error) => {
                if (error)
                    reject(asTransportError(error));
                else
                    resolve();
            });
        });
    }
    receive(options) {
        return this.documents.iterate(options);
    }
    close() {
        this.closePromise ??= new Promise((resolve) => {
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
    connect() {
        return new Promise((resolve, reject) => {
            let socket;
            try {
                socket = this.options.connect
                    ? this.options.connect()
                    : net.createConnection({ host: this.options.host, port: this.options.port });
            }
            catch (error) {
                reject(asTransportError(error));
                return;
            }
            this.socket = socket;
            let settled = false;
            const onConnect = () => {
                settled = true;
                resolve();
            };
            if (this.options.connect && !socket.connecting)
                onConnect();
            else
                socket.once('connect', onConnect);
            socket.on('data', (chunk) => {
                try {
                    for (const document of this.decoder.push(new Uint8Array(chunk))) {
                        this.documents.push(document);
                    }
                }
                catch (error) {
                    this.failWith(asTransportError(error));
                    socket.destroy();
                }
            });
            socket.on('error', (error) => {
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
                    if (!this.failure)
                        this.decoder.finish();
                    this.documents.end();
                }
                catch (error) {
                    this.failWith(asTransportError(error));
                }
            });
        });
    }
    failWith(error) {
        this.failure ??= error;
        this.documents.fail(error);
    }
}
export function createTcpTransport(options) {
    return new TcpTransport(options);
}
//# sourceMappingURL=tcp.js.map