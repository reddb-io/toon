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
import { TOON_RPC_CONTENT_TYPE } from './http.js';
import { DocumentQueue, abortError, asTransportError, raceSignal } from './internal.js';
export class SseTransportError extends Error {
    status;
    constructor(status, statusText) {
        super(`TOON-RPC SSE request failed: ${status} ${statusText}`.trimEnd());
        this.status = status;
        this.name = 'SseTransportError';
    }
}
export class SseTransport {
    kind = 'duplex';
    url;
    postUrl;
    headers;
    fetchImpl;
    documents = new DocumentQueue();
    lifetime = new AbortController();
    openPromise;
    pumpPromise;
    closed = false;
    failure;
    constructor(options) {
        this.url = String(options.url);
        this.postUrl = String(options.postUrl ?? options.url);
        this.headers = { ...options.headers };
        this.fetchImpl = options.fetch ?? fetch;
    }
    open(options) {
        this.openPromise ??= raceSignal(this.connect(), options?.signal);
        return this.openPromise;
    }
    async send(document, options) {
        await this.open(options);
        if (options?.signal?.aborted || this.lifetime.signal.aborted)
            throw abortError();
        if (this.failure)
            throw this.failure;
        const response = await this.fetchImpl(this.postUrl, {
            method: 'POST',
            body: document,
            headers: { 'Content-Type': TOON_RPC_CONTENT_TYPE, ...this.headers },
            signal: options?.signal ?? this.lifetime.signal,
        });
        await response.arrayBuffer().catch(() => undefined);
        if (!response.ok) {
            throw new SseTransportError(response.status, response.statusText ?? '');
        }
    }
    receive(options) {
        return this.documents.iterate(options);
    }
    async close() {
        if (this.closed)
            return;
        this.closed = true;
        this.documents.end();
        this.lifetime.abort(abortError());
        await this.pumpPromise?.catch(() => undefined);
    }
    async connect() {
        const response = await this.fetchImpl(this.url, {
            method: 'GET',
            headers: { Accept: 'text/event-stream', ...this.headers },
            signal: this.lifetime.signal,
        });
        if (!response.ok) {
            await response.arrayBuffer().catch(() => undefined);
            throw new SseTransportError(response.status, response.statusText ?? '');
        }
        if (!response.body)
            throw new Error('TOON-RPC SSE stream has no body');
        this.pumpPromise = this.pump(response.body);
    }
    async pump(body) {
        const reader = body.getReader();
        const parser = new SseEventParser((data) => {
            this.documents.push(new TextEncoder().encode(data));
        });
        try {
            for (;;) {
                const { done, value } = await reader.read();
                if (done)
                    break;
                if (value)
                    parser.push(value);
            }
            this.documents.end();
        }
        catch (error) {
            if (this.closed || this.lifetime.signal.aborted) {
                this.documents.end();
                return;
            }
            this.failure ??= asTransportError(error);
            this.documents.fail(this.failure);
        }
    }
}
export function createSseTransport(options) {
    return new SseTransport(options);
}
/** Minimal SSE parser: only complete events with data dispatch a document. */
class SseEventParser {
    onEvent;
    decoder = new TextDecoder('utf-8');
    buffer = '';
    dataLines = [];
    hasData = false;
    constructor(onEvent) {
        this.onEvent = onEvent;
    }
    push(chunk) {
        this.buffer += this.decoder.decode(chunk, { stream: true });
        for (;;) {
            const lineEnd = this.buffer.indexOf('\n');
            if (lineEnd === -1)
                return;
            let line = this.buffer.slice(0, lineEnd);
            this.buffer = this.buffer.slice(lineEnd + 1);
            if (line.endsWith('\r'))
                line = line.slice(0, -1);
            this.processLine(line);
        }
    }
    processLine(line) {
        if (line === '') {
            if (this.hasData)
                this.onEvent(this.dataLines.join('\n'));
            this.dataLines = [];
            this.hasData = false;
            return;
        }
        if (line.startsWith(':'))
            return;
        const colon = line.indexOf(':');
        const field = colon === -1 ? line : line.slice(0, colon);
        if (field !== 'data')
            return;
        let value = colon === -1 ? '' : line.slice(colon + 1);
        if (value.startsWith(' '))
            value = value.slice(1);
        this.dataLines.push(value);
        this.hasData = true;
    }
}
//# sourceMappingURL=sse.js.map