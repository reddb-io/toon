/**
 * HTTP transport for TOON-RPC
 */

export interface HttpTransportOptions {
  url: string;
  headers?: Record<string, string>;
}

export interface HttpTransport {
  send(data: Uint8Array): Promise<void>;
  recv(): AsyncIterable<Uint8Array>;
  close(): Promise<void>;
}

export function createHttpTransport(options: HttpTransportOptions): HttpTransport {
  return {
    async send(data: Uint8Array): Promise<void> {
      await fetch(options.url, {
        method: 'POST',
        body: data as BodyInit,
        headers: {
          'Content-Type': 'application/toon',
          ...options.headers,
        },
      });
    },
    async *recv(): AsyncIterable<Uint8Array> {
      // HTTP transport is request-response, no streaming recv
      throw new Error('HTTP transport does not support streaming recv');
    },
    async close(): Promise<void> {
      // No-op
    },
  };
}

/**
 * HTTP client transport (full request/response)
 */
export class HttpClient {
  private url: string;
  private headers: Record<string, string>;
  private buffer: Uint8Array | null = null;

  constructor(url: string, headers: Record<string, string> = {}) {
    this.url = url;
    this.headers = headers;
  }

  async send(data: Uint8Array): Promise<void> {
    const response = await fetch(this.url, {
      method: 'POST',
      body: data as BodyInit,
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

  async recv(): Promise<Uint8Array> {
    if (this.buffer === null) {
      throw new Error('No response received');
    }
    const data = this.buffer;
    this.buffer = null;
    return data;
  }
}
