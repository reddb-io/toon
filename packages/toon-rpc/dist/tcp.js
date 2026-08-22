/**
 * TCP transport for TOON-RPC (Node.js)
 */
import * as net from 'net';
export class TcpClient {
    host;
    port;
    socket = null;
    buffer = null;
    constructor(host, port) {
        this.host = host;
        this.port = port;
    }
    async connect() {
        return new Promise((resolve, reject) => {
            this.socket = net.createConnection({ host: this.host, port: this.port }, () => {
                resolve();
            });
            this.socket.on('error', reject);
        });
    }
    async send(data) {
        if (!this.socket) {
            await this.connect();
        }
        return new Promise((resolve, reject) => {
            const payload = Buffer.concat([data, Buffer.from('\n\n')]);
            this.socket.write(payload, (err) => {
                if (err)
                    reject(err);
                else
                    resolve();
            });
        });
    }
    async recv() {
        if (!this.socket) {
            throw new Error('Not connected');
        }
        if (this.buffer !== null) {
            const data = this.buffer;
            this.buffer = null;
            return data;
        }
        return new Promise((resolve, reject) => {
            let accumulator = Buffer.alloc(0);
            const onData = (chunk) => {
                accumulator = Buffer.concat([accumulator, chunk]);
                const idx = accumulator.indexOf('\n\n');
                if (idx !== -1) {
                    const message = accumulator.subarray(0, idx);
                    const remaining = accumulator.subarray(idx + 2);
                    this.socket.removeListener('data', onData);
                    if (remaining.length > 0) {
                        this.buffer = new Uint8Array(remaining);
                    }
                    resolve(new Uint8Array(message));
                }
            };
            this.socket.on('data', onData);
            this.socket.once('error', reject);
        });
    }
    async close() {
        if (this.socket) {
            this.socket.end();
            this.socket = null;
        }
    }
}
//# sourceMappingURL=tcp.js.map