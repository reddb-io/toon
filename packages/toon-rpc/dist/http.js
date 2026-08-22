/**
 * HTTP transport for TOON-RPC
 */
export function createHttpTransport(options) {
    return {
        async send(data) {
            await fetch(options.url, {
                method: 'POST',
                body: data,
                headers: {
                    'Content-Type': 'application/toon',
                    ...options.headers,
                },
            });
        },
        async *recv() {
            // HTTP transport is request-response, no streaming recv
            throw new Error('HTTP transport does not support streaming recv');
        },
        async close() {
            // No-op
        },
    };
}
/**
 * HTTP client transport (full request/response)
 */
export class HttpClient {
    url;
    headers;
    buffer = null;
    constructor(url, headers = {}) {
        this.url = url;
        this.headers = headers;
    }
    async send(data) {
        const response = await fetch(this.url, {
            method: 'POST',
            body: data,
            headers: {
                'Content-Type': 'application/toon',
                ...this.headers,
            },
        });
        if (!response.ok) {
            throw new Error(`HTTP error: ${response.status}`);
        }
        const buffer = await response.arrayBuffer();
        this.buffer = new Uint8Array(buffer);
    }
    async recv() {
        if (this.buffer === null) {
            throw new Error('No response received');
        }
        const data = this.buffer;
        this.buffer = null;
        return data;
    }
}
//# sourceMappingURL=http.js.map