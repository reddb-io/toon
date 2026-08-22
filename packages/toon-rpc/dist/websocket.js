/**
 * WebSocket transport for TOON-RPC
 */
export function createWebSocketTransport(url) {
    let ws = null;
    let buffer = null;
    async function* messageStream() {
        while (ws && ws.readyState === WebSocket.OPEN) {
            await new Promise((resolve) => {
                const handler = () => {
                    ws?.removeEventListener('message', handler);
                    resolve();
                };
                ws?.addEventListener('message', handler);
            });
            const data = ws && ws.__currentMessage;
            if (data) {
                ws.__currentMessage = null;
                yield data;
            }
        }
    }
    return {
        async send(data) {
            if (!ws) {
                ws = new WebSocket(url);
                ws.binaryType = 'arraybuffer';
                await new Promise((resolve, reject) => {
                    ws.onopen = () => resolve();
                    ws.onerror = (e) => reject(new Error(`WebSocket error: ${e}`));
                });
            }
            ws.send(data);
        },
        async *recv() {
            if (!buffer) {
                buffer = messageStream();
            }
            yield* buffer;
        },
        async close() {
            if (ws) {
                ws.close();
                ws = null;
            }
        },
    };
}
/**
 * Node.js WebSocket transport using `ws` package
 */
export class NodeWebSocketClient {
    url;
    ws = null;
    queue = [];
    waiters = [];
    constructor(url) {
        this.url = url;
    }
    async connect() {
        const WS = (await import('ws')).default;
        this.ws = new WS(this.url);
        await new Promise((resolve, reject) => {
            this.ws.on('open', () => resolve());
            this.ws.on('error', (e) => reject(e));
        });
        this.ws.on('message', (data) => {
            const bytes = new Uint8Array(data);
            if (this.waiters.length > 0) {
                const waiter = this.waiters.shift();
                waiter(bytes);
            }
            else {
                this.queue.push(bytes);
            }
        });
    }
    async send(data) {
        if (!this.ws) {
            await this.connect();
        }
        this.ws.send(data);
    }
    async recv() {
        if (this.queue.length > 0) {
            return this.queue.shift();
        }
        return new Promise((resolve) => {
            this.waiters.push(resolve);
        });
    }
    async close() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }
}
//# sourceMappingURL=websocket.js.map