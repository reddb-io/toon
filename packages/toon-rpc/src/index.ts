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

/** A response is exactly one of success or error — never both, never neither. */
export type Response = ResponseSuccess | ResponseError;

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
    let value: unknown;
    try {
      value = decode(text);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Parse error';
      return encodeResponse({
        toonrpc: TOONRPC_VERSION,
        error: { code: -32700, message: `Parse error: ${message}` },
        id: null,
      });
    }

    const isBatch = Array.isArray(value);
    const entries = isBatch ? (value as unknown[]) : [value];
    if (entries.length === 0) {
      return encodeResponse({
        toonrpc: TOONRPC_VERSION,
        error: { code: -32600, message: 'Invalid Request: empty batch' },
        id: null,
      });
    }

    const responses: Response[] = [];
    for (const entry of entries) {
      const response = await this.dispatchEntry(entry);
      if (response !== undefined) responses.push(response);
    }

    // All entries were notifications — nothing goes back on the wire.
    if (responses.length === 0) return new Uint8Array(0);
    const payload = isBatch ? responses : responses[0]!;
    return new TextEncoder().encode(encode(payload as unknown as JsonValue));
  }

  /**
   * Dispatch one already-parsed request entry.
   *
   * Returns the Response to send back, or `undefined` for a notification —
   * a request whose `id` is ABSENT. A present-but-`null` id is still an id
   * (discouraged, but legal), so it earns a response. Notifications run their
   * handler; only the answer is withheld, and a notification for an unknown
   * method or a throwing handler is dropped silently, as the spec requires.
   */
  async dispatchEntry(entry: unknown): Promise<Response | undefined> {
    if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) {
      return {
        toonrpc: TOONRPC_VERSION,
        error: { code: -32600, message: 'Invalid Request: not an object' },
        id: null,
      };
    }

    const record = entry as { method?: unknown; params?: unknown; id?: unknown };
    const isNotification = !('id' in record) || record.id === undefined;
    const id: Id =
      typeof record.id === 'string' || typeof record.id === 'number' ? record.id : null;

    if (typeof record.method !== 'string') {
      if (isNotification) return undefined;
      return {
        toonrpc: TOONRPC_VERSION,
        error: { code: -32600, message: 'Invalid Request: missing method' },
        id,
      };
    }

    const handler = this.methods.get(record.method);
    if (!handler) {
      if (isNotification) return undefined;
      return {
        toonrpc: TOONRPC_VERSION,
        error: { code: -32601, message: 'Method not found' },
        id,
      };
    }

    try {
      const result = await handler((record.params ?? {}) as Params, id);
      if (isNotification) return undefined;
      return { toonrpc: TOONRPC_VERSION, result, id };
    } catch (err) {
      if (isNotification) return undefined;
      if (err instanceof RpcError) {
        return {
          toonrpc: TOONRPC_VERSION,
          error: { code: err.code, message: err.message, ...(err.data === undefined ? {} : { data: err.data }) },
          id,
        };
      }
      return {
        toonrpc: TOONRPC_VERSION,
        error: { code: -32603, message: err instanceof Error ? err.message : 'Internal error' },
        id,
      };
    }
  }
}

function encodeResponse(response: Response): Uint8Array {
  return new TextEncoder().encode(encode(response as unknown as JsonValue));
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
