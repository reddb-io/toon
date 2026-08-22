import { decode, encode } from '@reddb-io/toon';
export const TOONRPC_VERSION = '1.0';
export class RpcError extends Error {
    code;
    data;
    constructor(code, message, data) {
        super(message);
        this.code = code;
        this.data = data;
        this.name = 'RpcError';
    }
}
export class Server {
    methods = new Map();
    constructor() { }
    register(method, handler) {
        this.methods.set(method, handler);
    }
    async handle(raw) {
        const text = new TextDecoder().decode(raw);
        return this.handleText(text);
    }
    async handleText(text) {
        let value;
        try {
            value = decode(text);
        }
        catch (err) {
            const message = err instanceof Error ? err.message : 'Parse error';
            return encodeResponse({
                toonrpc: TOONRPC_VERSION,
                error: { code: -32700, message: `Parse error: ${message}` },
                id: null,
            });
        }
        const isBatch = Array.isArray(value);
        const entries = isBatch ? value : [value];
        if (entries.length === 0) {
            return encodeResponse({
                toonrpc: TOONRPC_VERSION,
                error: { code: -32600, message: 'Invalid Request: empty batch' },
                id: null,
            });
        }
        const responses = [];
        for (const entry of entries) {
            const response = await this.dispatchEntry(entry);
            if (response !== undefined)
                responses.push(response);
        }
        // All entries were notifications — nothing goes back on the wire.
        if (responses.length === 0)
            return new Uint8Array(0);
        const payload = isBatch ? responses : responses[0];
        return new TextEncoder().encode(encode(payload));
    }
    /**
     * Dispatch one already-parsed request entry.
     *
     * Returns the Response to send back, or `undefined` for a notification —
     * a request whose `id` is ABSENT. A present-but-`null` id is still an id
     * (discouraged, but legal), so it earns a response. Notifications run their
     * handler; only the answer is withheld, and a notification for an unknown
     * method or a throwing handler is dropped silently, as the spec requires.
     */
    async dispatchEntry(entry) {
        if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) {
            return {
                toonrpc: TOONRPC_VERSION,
                error: { code: -32600, message: 'Invalid Request: not an object' },
                id: null,
            };
        }
        const record = entry;
        const isNotification = !('id' in record) || record.id === undefined;
        const id = typeof record.id === 'string' || typeof record.id === 'number' ? record.id : null;
        if (typeof record.method !== 'string') {
            if (isNotification)
                return undefined;
            return {
                toonrpc: TOONRPC_VERSION,
                error: { code: -32600, message: 'Invalid Request: missing method' },
                id,
            };
        }
        const handler = this.methods.get(record.method);
        if (!handler) {
            if (isNotification)
                return undefined;
            return {
                toonrpc: TOONRPC_VERSION,
                error: { code: -32601, message: 'Method not found' },
                id,
            };
        }
        try {
            const result = await handler((record.params ?? {}), id);
            if (isNotification)
                return undefined;
            return { toonrpc: TOONRPC_VERSION, result, id };
        }
        catch (err) {
            if (isNotification)
                return undefined;
            if (err instanceof RpcError) {
                return {
                    toonrpc: TOONRPC_VERSION,
                    error: { code: err.code, message: err.message, ...(err.data === undefined ? {} : { data: err.data }) },
                    id,
                };
            }
            return {
                toonrpc: TOONRPC_VERSION,
                error: { code: -32603, message: err instanceof Error ? err.message : 'Internal error' },
                id,
            };
        }
    }
}
function encodeResponse(response) {
    return new TextEncoder().encode(encode(response));
}
export class Client {
    transport;
    idCounter = 0;
    pending = new Map();
    constructor(transport) {
        this.transport = transport;
    }
    async call(method, params) {
        const id = this.idCounter++;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            const request = {
                toonrpc: TOONRPC_VERSION,
                method,
                params,
                id,
            };
            const toonInput = encode(request);
            this.transport
                .send(new TextEncoder().encode(toonInput))
                .catch((err) => {
                this.pending.delete(id);
                reject(err);
            });
        });
    }
    async *recv() {
        for await (const chunk of this.transport.recv()) {
            const text = new TextDecoder().decode(chunk);
            const lines = text.split('\n').filter((l) => l.trim());
            for (const line of lines) {
                const toonValue = decode(line);
                const resp = toonValue;
                const pending = this.pending.get(resp.id);
                if (pending) {
                    this.pending.delete(resp.id);
                    if ('error' in resp) {
                        pending.reject(new RpcError(resp.error.code, resp.error.message, resp.error.data));
                    }
                    else {
                        pending.resolve(resp.result);
                    }
                }
            }
        }
    }
    close() {
        return this.transport.close();
    }
}
export function createStdioTransport() {
    return {
        async send(data) {
            const text = new TextDecoder().decode(data);
            process.stdout.write(text);
            if (!text.endsWith('\n')) {
                process.stdout.write('\n');
            }
        },
        async *recv() {
            const stdin = process.stdin;
            for await (const chunk of stdin) {
                yield new TextEncoder().encode(chunk);
            }
        },
        async close() {
            process.stdin.pause();
        },
    };
}
//# sourceMappingURL=index.js.map