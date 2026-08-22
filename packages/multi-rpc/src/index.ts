/**
 * Multi-protocol RPC: auto-detects JSON-RPC 2.0 vs TOON-RPC 1.0 on the wire
 * and answers in the same format the client used.
 */

import { decode, encode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';
import { Server, TOONRPC_VERSION } from '@reddb-io/toon-rpc';
import type { Id, Params } from '@reddb-io/toon-rpc';

export { Server } from '@reddb-io/toon-rpc';

export const JSONRPC_VERSION = '2.0';

/** Wire protocol variants the dispatcher can negotiate. */
export type Protocol = 'jsonrpc' | 'toonrpc';

/** MIME type for HTTP `Content-Type` / `Accept` negotiation. */
export function contentTypeFor(protocol: Protocol): string {
  return protocol === 'jsonrpc' ? 'application/json' : 'application/toon';
}

/** Detect the protocol from a content-type hint and/or raw bytes. */
export function detectProtocol(raw: Uint8Array | string, contentType?: string): Protocol {
  if (contentType !== undefined) {
    const lower = contentType.toLowerCase();
    if (lower.includes('application/json')) return 'jsonrpc';
    if (lower.includes('application/toon')) return 'toonrpc';
  }

  const text = typeof raw === 'string' ? raw : new TextDecoder().decode(raw);
  const trimmed = text.trimStart();

  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    // An inline TOON object may also start with `{`; only the JSON-RPC marker
    // makes that prefix unambiguous.
    if (trimmed.slice(0, 80).includes('"jsonrpc"')) return 'jsonrpc';
  }

  return 'toonrpc';
}

interface WireEntry {
  method?: unknown;
  params?: unknown;
  id?: unknown;
  jsonrpc?: unknown;
  toonrpc?: unknown;
}

/** A single method registry behind JSON-RPC and TOON-RPC wire formats. */
export class MultiRpc {
  constructor(private server: Server) {}

  async handle(raw: Uint8Array | string, contentType?: string): Promise<Uint8Array> {
    const { body } = await this.handleWithProtocol(raw, contentType);
    return body;
  }

  async handleWithProtocol(
    raw: Uint8Array | string,
    contentType?: string
  ): Promise<{ protocol: Protocol; body: Uint8Array }> {
    const protocol = detectProtocol(raw, contentType);
    const text = typeof raw === 'string' ? raw : new TextDecoder().decode(raw);
    const body =
      protocol === 'jsonrpc' ? await this.handleJsonRpc(text) : await this.server.handleText(text);
    return { protocol, body };
  }

  private async handleJsonRpc(text: string): Promise<Uint8Array> {
    let value: unknown;
    try {
      value = JSON.parse(text);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Parse error';
      return jsonBytes({
        jsonrpc: JSONRPC_VERSION,
        error: { code: -32700, message: `Parse error: ${message}` },
        id: null,
      });
    }

    const isBatch = Array.isArray(value);
    const entries = (isBatch ? value : [value]) as WireEntry[];
    if (entries.length === 0) {
      return jsonBytes({
        jsonrpc: JSONRPC_VERSION,
        error: { code: -32600, message: 'Invalid Request: empty batch' },
        id: null,
      });
    }

    const responses: JsonValue[] = [];
    for (const entry of entries) {
      const response = await this.dispatchJsonRpcEntry(entry);
      if (response !== undefined) responses.push(response);
    }

    if (responses.length === 0) return new Uint8Array(0);
    return jsonBytes(isBatch ? responses : responses[0]!);
  }

  private async dispatchJsonRpcEntry(entry: WireEntry): Promise<JsonValue | undefined> {
    if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) {
      return {
        jsonrpc: JSONRPC_VERSION,
        error: { code: -32600, message: 'Invalid Request: not an object' },
        id: null,
      };
    }

    if (entry.jsonrpc !== JSONRPC_VERSION) {
      return {
        jsonrpc: JSONRPC_VERSION,
        error: { code: -32600, message: `Invalid Request: expected jsonrpc "${JSONRPC_VERSION}"` },
        id: idOf(entry),
      };
    }

    const dispatched = await this.server.dispatchEntry({
      toonrpc: TOONRPC_VERSION,
      method: entry.method,
      params: entry.params,
      id: entry.id,
    });
    if (dispatched === undefined) return undefined;

    const { toonrpc: _drop, ...rest } = dispatched as unknown as Record<string, JsonValue>;
    return { jsonrpc: JSONRPC_VERSION, ...rest };
  }
}

function idOf(entry: WireEntry): Id {
  const id = entry.id;
  return typeof id === 'string' || typeof id === 'number' ? id : null;
}

function jsonBytes(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}

/** Re-encode an already-parsed RPC message in the named dialect. */
export function encodeMessage(message: Record<string, JsonValue>, protocol: Protocol): string {
  const { jsonrpc: _j, toonrpc: _t, ...rest } = message as Record<string, JsonValue>;
  if (protocol === 'jsonrpc') {
    return JSON.stringify({ jsonrpc: JSONRPC_VERSION, ...rest });
  }
  return encode({ toonrpc: TOONRPC_VERSION, ...rest } as JsonValue);
}

/** Decode a framed message into the JSON-RPC-compatible object shape. */
export function decodeMessage(frame: string): {
  message: Record<string, JsonValue>;
  protocol: Protocol;
} {
  const protocol: Protocol = frame.trimStart().startsWith('{') ? 'jsonrpc' : 'toonrpc';
  const raw = (protocol === 'jsonrpc' ? JSON.parse(frame) : decode(frame)) as Record<
    string,
    JsonValue
  >;
  const { jsonrpc: _j, toonrpc: _t, ...rest } = raw;
  return { message: { jsonrpc: JSONRPC_VERSION, ...rest }, protocol };
}

export type { Id, Params };
