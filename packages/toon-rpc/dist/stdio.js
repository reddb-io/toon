/**
 * stdio transport for TOON-RPC (Node.js).
 *
 * A stdin/stdout pipe is a byte stream, so this transport speaks the
 * length-prefixed stream framing profile from `framing.ts` — one frame per
 * complete RPC document, reassembled across arbitrary chunk splits. The
 * streams are injectable for tests and for embedding a child process's
 * pipes; they default to this process's stdin/stdout.
 */
import { FrameDecoder, encodeFrame } from './framing.js';
import { DocumentQueue, abortError, asTransportError } from './internal.js';
export class StdioTransport {
    kind = 'duplex';
    input;
    output;
    documents = new DocumentQueue();
    decoder = new FrameDecoder();
    started = false;
    closed = false;
    failure;
    constructor(options = {}) {
        this.input = options.input ?? process.stdin;
        this.output = options.output ?? process.stdout;
    }
    async open() {
        if (this.started)
            return;
        this.started = true;
        this.input.on('data', (chunk) => {
            const bytes = typeof chunk === 'string' ? new TextEncoder().encode(chunk) : new Uint8Array(chunk);
            try {
                for (const document of this.decoder.push(bytes)) {
                    this.documents.push(document);
                }
            }
            catch (error) {
                this.failWith(asTransportError(error));
            }
        });
        this.input.on('error', (error) => this.failWith(asTransportError(error)));
        this.input.on('end', () => {
            try {
                if (!this.failure)
                    this.decoder.finish();
                this.documents.end();
            }
            catch (error) {
                this.failWith(asTransportError(error));
            }
        });
    }
    async send(document, options) {
        await this.open();
        if (options?.signal?.aborted)
            throw abortError();
        if (this.failure)
            throw this.failure;
        if (this.closed)
            throw new Error('TOON-RPC stdio transport is closed');
        await new Promise((resolve, reject) => {
            this.output.write(encodeFrame(document), (error) => {
                if (error)
                    reject(asTransportError(error));
                else
                    resolve();
            });
        });
    }
    receive(options) {
        void this.open();
        return this.documents.iterate(options);
    }
    async close() {
        if (this.closed)
            return;
        this.closed = true;
        this.documents.end();
        this.input.pause?.();
    }
    failWith(error) {
        this.failure ??= error;
        this.documents.fail(error);
    }
}
export function createStdioTransport(options = {}) {
    return new StdioTransport(options);
}
//# sourceMappingURL=stdio.js.map