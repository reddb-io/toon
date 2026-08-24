import { decode, encode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';
import {
  TOONRPC_VERSION,
  isUnicodeScalarString,
  snapshotCoreValue,
  snapshotErrorObject,
  snapshotRequestObject,
  snapshotResponse,
} from './protocol.js';
import type {
  CoreValue,
  ErrorObject,
  Id,
  Params,
  Request,
  Response,
  ResponseError,
  ResponseSuccess,
} from './protocol.js';

export * from './protocol.js';

export interface Transport {
  send(data: Uint8Array): Promise<void>;
  recv(): AsyncIterable<Uint8Array>;
  close(): Promise<void>;
}

export class RpcError extends Error {
  readonly code: number;
  readonly data: CoreValue | undefined;
  readonly hasData: boolean;

  constructor(code: number, message: string);
  constructor(code: number, message: string, data: CoreValue);
  constructor(code: number, message: string, data?: CoreValue) {
    super(message);
    this.name = 'RpcError';
    this.code = code;
    this.data = data;
    this.hasData = arguments.length >= 3;
  }
}

export interface MethodHandler {
  (params: Params | undefined, id: Id | undefined): Promise<CoreValue>;
}

export class Server {
  private methods = new Map<string, MethodHandler>();

  constructor() {}

  register(method: string, handler: MethodHandler): void {
    this.methods.set(method, handler);
  }

  async handle(raw: Uint8Array): Promise<Uint8Array> {
    let text: string;
    try {
      text = new TextDecoder('utf-8', { fatal: true }).decode(raw);
    } catch {
      return encodeResponse(parseError('Input is not valid UTF-8'));
    }
    return this.handleText(text);
  }

  async handleText(text: string): Promise<Uint8Array> {
    if (!isUnicodeScalarString(text)) {
      return encodeResponse(parseError('Input contains an invalid Unicode surrogate'));
    }

    let value: unknown;
    try {
      value = decode(text);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Invalid TOON';
      return encodeResponse(parseError(message));
    }

    const isBatch = Array.isArray(value);
    const entries = isBatch ? (value as unknown[]) : [value];
    if (entries.length === 0) {
      return encodeResponse(invalidRequest('empty batch'));
    }

    const responses: Response[] = [];
    for (const entry of entries) {
      const response = await this.dispatchEntry(entry);
      if (response !== undefined) responses.push(preflightToonResponse(response, isBatch));
    }

    // All entries were notifications — nothing goes back on the wire.
    if (responses.length === 0) return new Uint8Array(0);
    return isBatch ? encodeToonBatch(responses) : encodeResponse(responses[0]!);
  }

  /**
   * Dispatch one already-parsed request entry.
   *
   * Returns the response to send, or `undefined` for a valid notification.
   * Malformed entries always produce an uncorrelated Invalid Request response.
   */
  async dispatchEntry(entry: unknown): Promise<Response | undefined> {
    const request = snapshotRequestObject(entry);
    if (!request) return invalidRequest('invalid envelope');

    const notification = !Object.prototype.hasOwnProperty.call(request, 'id');
    const id = notification ? undefined : request.id;
    const handler = this.methods.get(request.method);
    if (!handler) {
      if (notification) return undefined;
      return {
        toonrpc: TOONRPC_VERSION,
        error: { code: -32601, message: 'Method not found' },
        id: id!,
      };
    }

    try {
      const result = await handler(request.params, id);
      if (notification) return undefined;
      const snapshot = snapshotCoreValue(result);
      if (snapshot === undefined) return internalError(id!);
      return { toonrpc: TOONRPC_VERSION, result: snapshot, id: id! };
    } catch (err) {
      if (notification) return undefined;
      const error = snapshotHandlerError(err);
      return error
        ? { toonrpc: TOONRPC_VERSION, error, id: id! }
        : internalError(id!);
    }
  }
}

function parseError(detail: string): ResponseError {
  return {
    toonrpc: TOONRPC_VERSION,
    error: { code: -32700, message: `Parse error: ${detail}` },
    id: null,
  };
}

function invalidRequest(detail: string): ResponseError {
  return {
    toonrpc: TOONRPC_VERSION,
    error: { code: -32600, message: `Invalid Request: ${detail}` },
    id: null,
  };
}

function internalError(id: Id): ResponseError {
  return {
    toonrpc: TOONRPC_VERSION,
    error: { code: -32603, message: 'Internal error' },
    id,
  };
}

function snapshotHandlerError(value: unknown): ErrorObject | undefined {
  try {
    if (!(value instanceof RpcError)) return undefined;
    const code = Object.getOwnPropertyDescriptor(value, 'code');
    const message = Object.getOwnPropertyDescriptor(value, 'message');
    const hasData = Object.getOwnPropertyDescriptor(value, 'hasData');
    if (!code || !('value' in code) || !message || !('value' in message)) return undefined;
    if (!hasData || !('value' in hasData) || typeof hasData.value !== 'boolean') return undefined;
    if (!isHandlerErrorCode(code.value)) return undefined;

    const source: Record<string, unknown> = { code: code.value, message: message.value };
    if (hasData.value) {
      const data = Object.getOwnPropertyDescriptor(value, 'data');
      if (!data || !('value' in data)) return undefined;
      source.data = data.value;
    }
    return snapshotErrorObject(source);
  } catch {
    return undefined;
  }
}

function isHandlerErrorCode(code: unknown): code is number {
  if (
    typeof code !== 'number' ||
    !Number.isInteger(code) ||
    code < -2147483648 ||
    code > 2147483647
  ) {
    return false;
  }
  return (
    code === -32602 ||
    code === -32603 ||
    (code >= -32099 && code <= -32000) ||
    code < -32768 ||
    code > -32000
  );
}

function preflightToonResponse(response: Response, batch: boolean): Response {
  const snapshot = snapshotResponse(response);
  if (snapshot !== undefined) {
    try {
      encode((batch ? toonBatchProbe(snapshot) : snapshot) as unknown as JsonValue);
      return snapshot;
    } catch {
      // Fall through to a shallow, correlated Internal Error.
    }
  }

  const fallback = snapshotResponse(internalError(response.id))!;
  encode((batch ? toonBatchProbe(fallback) : fallback) as unknown as JsonValue);
  return fallback;
}

function toonBatchProbe(response: Response): Response[] {
  // Mixed branch shapes force the recursive list form and include the batch
  // root depth, avoiding the shallower uniform-table path used by [response].
  return [
    response,
    { toonrpc: TOONRPC_VERSION, result: { probe: 'success' }, id: '__preflight_success__' },
    {
      toonrpc: TOONRPC_VERSION,
      error: { code: -32603, message: 'Internal error' },
      id: '__preflight_error__',
    },
  ];
}

function encodeToonBatch(responses: Response[]): Uint8Array {
  try {
    return encodeResponse(responses);
  } catch {
    const stable: Response[] = [];
    for (const response of responses) {
      const candidate = [...stable, response];
      try {
        encode(candidate as unknown as JsonValue);
        stable.push(response);
      } catch {
        const fallback = snapshotResponse(internalError(response.id))!;
        stable.push(fallback);
        encode(stable as unknown as JsonValue);
      }
    }
    return encodeResponse(stable);
  }
}

function encodeResponse(response: Response | Response[]): Uint8Array {
  return new TextEncoder().encode(encode(response as unknown as JsonValue));
}

export class Client {
  private idCounter = 0;
  private pending = new Map<Id, { resolve: (v: CoreValue) => void; reject: (e: Error) => void }>();

  constructor(private transport: Transport) {}

  async call(method: string, params: Params): Promise<CoreValue> {
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
            pending.reject(
              Object.prototype.hasOwnProperty.call(resp.error, 'data')
                ? new RpcError(resp.error.code, resp.error.message, resp.error.data!)
                : new RpcError(resp.error.code, resp.error.message)
            );
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
