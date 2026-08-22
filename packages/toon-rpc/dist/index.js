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
        const toonValue = decode(text);
        const req = toonValue;
        const responses = await this.dispatch(req);
        if (responses.length === 0) {
            return new Uint8Array(0);
        }
        const response = responses.length === 1 ? responses[0] : responses;
        const toonOutput = encode(response);
        return new TextEncoder().encode(toonOutput);
    }
    async dispatch(req) {
        const requests = Array.isArray(req) ? req : [req];
        const responses = [];
        for (const r of requests) {
            if (r.id === undefined || r.id === null) {
                continue;
            }
            const handler = this.methods.get(r.method);
            if (!handler) {
                responses.push({
                    toonrpc: TOONRPC_VERSION,
                    error: { code: -32601, message: 'Method not found' },
                    id: r.id,
                });
                continue;
            }
            try {
                const result = await handler(r.params, r.id);
                responses.push({
                    toonrpc: TOONRPC_VERSION,
                    result,
                    id: r.id,
                });
            }
            catch (err) {
                responses.push({
                    toonrpc: TOONRPC_VERSION,
                    error: {
                        code: -32603,
                        message: err instanceof Error ? err.message : 'Internal error',
                    },
                    id: r.id,
                });
            }
        }
        return responses;
    }
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