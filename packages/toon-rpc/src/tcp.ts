/**
 * TCP transport for TOON-RPC (Node.js)
 */

import * as net from 'net';

export class TcpClient {
  private host: string;
  private port: number;
  private socket: net.Socket | null = null;
  private buffer: Uint8Array | null = null;

  constructor(host: string, port: number) {
    this.host = host;
    this.port = port;
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.socket = net.createConnection({ host: this.host, port: this.port }, () => {
        resolve();
      });
      this.socket.on('error', reject);
    });
  }

  async send(data: Uint8Array): Promise<void> {
    if (!this.socket) {
      await this.connect();
    }
    return new Promise((resolve, reject) => {
      const payload = Buffer.concat([data, Buffer.from('\n\n')]);
      this.socket!.write(payload, (err) => {
        if (err) reject(err);
        else resolve();
      });
    });
  }

  async recv(): Promise<Uint8Array> {
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

      const onData = (chunk: Buffer) => {
        accumulator = Buffer.concat([accumulator, chunk]);
        const idx = accumulator.indexOf('\n\n');
        if (idx !== -1) {
          const message = accumulator.subarray(0, idx);
          const remaining = accumulator.subarray(idx + 2);
          this.socket!.removeListener('data', onData);

          if (remaining.length > 0) {
            this.buffer = new Uint8Array(remaining);
          }

          resolve(new Uint8Array(message));
        }
      };

      this.socket!.on('data', onData);
      this.socket!.once('error', reject);
    });
  }

  async close(): Promise<void> {
    if (this.socket) {
      this.socket.end();
      this.socket = null;
    }
  }
}
