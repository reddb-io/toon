/**
 * Server-Sent Events (SSE) transport for TOON-RPC
 */

import type { Transport } from './index';

export class SseClient {
  private url: string;
  private eventSource: EventSource | null = null;
  private queue: Uint8Array[] = [];
  private waiters: ((data: Uint8Array) => void)[] = [];

  constructor(url: string) {
    this.url = url;
  }

  async connect(): Promise<void> {
    if (typeof EventSource === 'undefined') {
      throw new Error('EventSource is not available (browser only)');
    }
    this.eventSource = new EventSource(this.url);
    await new Promise<void>((resolve, reject) => {
      this.eventSource!.onopen = () => resolve();
      this.eventSource!.onerror = (e) => reject(new Error(`SSE error: ${e}`));
    });

    this.eventSource.onmessage = (event) => {
      const data = new TextEncoder().encode(event.data);
      if (this.waiters.length > 0) {
        const waiter = this.waiters.shift()!;
        waiter(data);
      } else {
        this.queue.push(data);
      }
    };
  }

  async send(data: Uint8Array): Promise<void> {
    const response = await fetch(this.url.replace('/sse', '/rpc'), {
      method: 'POST',
      body: data as BodyInit,
      headers: {
        'Content-Type': 'application/toon',
      },
    });
    if (!response.ok) {
      throw new Error(`SSE send error: ${response.status}`);
    }
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
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }
}
