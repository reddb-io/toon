/**
 * WebSocket transport for TOON-RPC
 */

import type { Transport } from './index';

export function createWebSocketTransport(url: string): Transport {
  let ws: WebSocket | null = null;
  let buffer: AsyncIterableIterator<Uint8Array> | null = null;

  async function* messageStream(): AsyncIterableIterator<Uint8Array> {
    while (ws && ws.readyState === WebSocket.OPEN) {
      await new Promise<void>((resolve) => {
        const handler = () => {
          ws?.removeEventListener('message', handler);
          resolve();
        };
        ws?.addEventListener('message', handler);
      });

      const data = ws && (ws as any).__currentMessage;
      if (data) {
        (ws as any).__currentMessage = null;
        yield data;
      }
    }
  }

  return {
    async send(data: Uint8Array): Promise<void> {
      if (!ws) {
        ws = new WebSocket(url);
        ws.binaryType = 'arraybuffer';

        await new Promise<void>((resolve, reject) => {
          ws!.onopen = () => resolve();
          ws!.onerror = (e) => reject(new Error(`WebSocket error: ${e}`));
        });
      }

      ws.send(data);
    },

    async *recv(): AsyncIterable<Uint8Array> {
      if (!buffer) {
        buffer = messageStream();
      }
      yield* buffer;
    },

    async close(): Promise<void> {
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
  private url: string;
  private ws: any = null;
  private queue: Uint8Array[] = [];
  private waiters: ((data: Uint8Array) => void)[] = [];

  constructor(url: string) {
    this.url = url;
  }

  async connect(): Promise<void> {
    const WS = (await import('ws')).default;
    this.ws = new WS(this.url);

    await new Promise<void>((resolve, reject) => {
      this.ws!.on('open', () => resolve());
      this.ws!.on('error', (e: Error) => reject(e));
    });

    this.ws.on('message', (data: Buffer) => {
      const bytes = new Uint8Array(data);
      if (this.waiters.length > 0) {
        const waiter = this.waiters.shift()!;
        waiter(bytes);
      } else {
        this.queue.push(bytes);
      }
    });
  }

  async send(data: Uint8Array): Promise<void> {
    if (!this.ws) {
      await this.connect();
    }
    this.ws.send(data);
  }

  async recv(): Promise<Uint8Array> {
    if (this.queue.length > 0) {
      return this.queue.shift()!;
    }
    return new Promise((resolve) => {
      this.waiters.push(resolve);
    });
  }

  async close(): Promise<void> {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}
