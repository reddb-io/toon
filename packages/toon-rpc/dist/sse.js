/**
 * Server-Sent Events (SSE) transport for TOON-RPC
 */
export class SseClient {
    url;
    eventSource = null;
    queue = [];
    waiters = [];
    constructor(url) {
        this.url = url;
    }
    async connect() {
        if (typeof EventSource === 'undefined') {
            throw new Error('EventSource is not available (browser only)');
        }
        this.eventSource = new EventSource(this.url);
        await new Promise((resolve, reject) => {
            this.eventSource.onopen = () => resolve();
            this.eventSource.onerror = (e) => reject(new Error(`SSE error: ${e}`));
        });
        this.eventSource.onmessage = (event) => {
            const data = new TextEncoder().encode(event.data);
            if (this.waiters.length > 0) {
                const waiter = this.waiters.shift();
                waiter(data);
            }
            else {
                this.queue.push(data);
            }
        };
    }
    async send(data) {
        const response = await fetch(this.url.replace('/sse', '/rpc'), {
            method: 'POST',
            body: data,
            headers: {
                'Content-Type': 'application/toon',
            },
        });
        if (!response.ok) {
            throw new Error(`SSE send error: ${response.status}`);
        }
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
        if (this.eventSource) {
            this.eventSource.close();
            this.eventSource = null;
        }
    }
}
//# sourceMappingURL=sse.js.map