import { decode, encode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';

export const TOONRPC_VERSION = '1.0';

export interface Request {
  toonrpc: '1.0';
  method: string;
  params: Params;
  id: Id;
}

export interface Notification {
  toonrpc: '1.0';
  method: string;
  params: Params;
}

export interface ResponseSuccess {
  toonrpc: '1.0';
  result: JsonValue;
  id: Id;
}

export interface ResponseError {
  toonrpc: '1.0';
  error: ErrorObject;
  id: Id;
}

export interface ErrorObject {
  code: number;
  message: string;
  data?: JsonValue;
}

export type Id = string | number | null;
export type Params = JsonValue[] | Record<string, JsonValue>;

export interface Transport {
  send(data: Uint8Array): Promise<void>;
  recv(): AsyncIterable<Uint8Array>;
  close(): Promise<void>;
}

export class RpcError extends Error {
  constructor(
    public code: number,
    message: string,
    public data?: JsonValue
  ) {
    super(message);
    this.name = 'RpcError';
  }
}

export interface MethodHandler {
  (params: Params, id: Id): Promise<JsonValue>;
}

export class Server {
  private methods = new Map<string, MethodHandler>();

  constructor() {}

  register(method: string, handler: MethodHandler): void {
    this.methods.set(method, handler);
  }

  async handle(raw: Uint8Array): Promise<Uint8Array> {
    const text = new TextDecoder().decode(raw);
    return this.handleText(text);
  }

  async handleText(text: string): Promise<Uint8Array> {
    const toonValue = decode(text);
    const req = toonValue as unknown as Request | Request[];

    const responses = await this.dispatch(req);
    if (responses.length === 0) {
      return new Uint8Array(0);
    }
    const response = responses.length === 1 ? responses[0] : responses;
    const toonOutput = encode(response as unknown as JsonValue);
    return new TextEncoder().encode(toonOutput);
  }

  private async dispatch(req: Request | Request[]): Promise<ResponseSuccess[]> {
    const requests = Array.isArray(req) ? req : [req];
    const responses: ResponseSuccess[] = [];

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
        } as unknown as ResponseSuccess);
        continue;
      }

      try {
        const result = await handler(r.params, r.id);
        responses.push({
          toonrpc: TOONRPC_VERSION,
          result,
          id: r.id,
        });
      } catch (err) {
        responses.push({
          toonrpc: TOONRPC_VERSION,
          error: {
            code: -32603,
            message: err instanceof Error ? err.message : 'Internal error',
          },
          id: r.id,
        } as unknown as ResponseSuccess);
      }
    }

    return responses;
  }
}

export class Client {
  private idCounter = 0;
  private pending = new Map<Id, { resolve: (v: JsonValue) => void; reject: (e: Error) => void }>();

  constructor(private transport: Transport) {}

  async call(method: string, params: Params): Promise<JsonValue> {
    const id = this.idCounter++;

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });

      const request: Request = {
        toonrpc: TOONRPC_VERSION,
        method,
        params,
        id,
      };

      const toonInput = encode(request as unknown as JsonValue);
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
        const resp = toonValue as unknown as ResponseSuccess | ResponseError;
        const pending = this.pending.get(resp.id);
        if (pending) {
          this.pending.delete(resp.id);
          if ('error' in resp) {
            pending.reject(new RpcError(resp.error.code, resp.error.message, resp.error.data));
          } else {
            pending.resolve(resp.result);
          }
        }
      }
    }
  }

  close(): Promise<void> {
    return this.transport.close();
  }
}

export function createStdioTransport(): Transport {
  return {
    async send(data: Uint8Array): Promise<void> {
      const text = new TextDecoder().decode(data);
      process.stdout.write(text);
      if (!text.endsWith('\n')) {
        process.stdout.write('\n');
      }
    },
    async *recv(): AsyncIterable<Uint8Array> {
      const stdin = process.stdin;
      for await (const chunk of stdin) {
        yield new TextEncoder().encode(chunk);
      }
    },
    async close(): Promise<void> {
      process.stdin.pause();
    },
  };
}
