/**
 * Multi-protocol RPC — auto-detects JSON-RPC 2.0 vs TOON-RPC 1.0 on the wire
 * and answers in the same format the client used.
 *
 * TypeScript port of the Rust `reddb_io_toon_rpc::multi` module; the detection
 * rules are the same on both sides so a mixed fleet cannot disagree about what
 * a request was:
 *
 * - An explicit `Content-Type: application/json` or `application/toon` wins.
 * - A body starting with `{` or `[` whose head contains `"jsonrpc"` is JSON-RPC.
 * - A body starting with `toonrpc:` or `{toonrpc` is TOON-RPC.
 * - Anything else is TOON-RPC — the preferred format.
 */

import { decode, encode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';
import { Server, TOONRPC_VERSION } from './index.js';
import type { Id, Params } from './index.js';

export const JSONRPC_VERSION = '2.0';

/** Wire protocol variants the dispatcher can negotiate. */
export type Protocol = 'jsonrpc' | 'toonrpc';

/** MIME type for HTTP `Content-Type` / `Accept` negotiation. */
export function contentTypeFor(protocol: Protocol): string {
  return protocol === 'jsonrpc' ? 'application/json' : 'application/toon';
}

/**
 * Detect the protocol from a content-type hint and/or raw bytes.
 *
 * An explicit content-type hint (when provided) wins over byte sniffing.
 */
export function detectProtocol(raw: Uint8Array | string, contentType?: string): Protocol {
  if (contentType !== undefined) {
    const lower = contentType.toLowerCase();
    if (lower.includes('application/json')) return 'jsonrpc';
    if (lower.includes('application/toon')) return 'toonrpc';
  }

  const text = typeof raw === 'string' ? raw : new TextDecoder().decode(raw);
  const trimmed = text.trimStart();

  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    // JSON-RPC bodies (single or batch) carry `"jsonrpc"` within the first
    // ~80 bytes. A `{`-headed body without it could still be TOON's inline
    // object form, so absence keeps the preferred default.
    if (trimmed.slice(0, 80).includes('"jsonrpc"')) return 'jsonrpc';
    return 'toonrpc';
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

/**
 * Multi-protocol dispatcher — a single method registry behind two wire formats.
 *
 * `handle` detects the dialect of each request and answers in kind, so a
 * JSON-RPC client and a TOON-RPC client can share one endpoint without either
 * being told about the other.
 */
export class MultiRpc {
  constructor(private server: Server) {}

  async handle(raw: Uint8Array | string, contentType?: string): Promise<Uint8Array> {
    const { body } = await this.handleWithProtocol(raw, contentType);
    return body;
  }

  /**
   * Handle a request, returning the detected protocol alongside the response
   * bytes — for transports that need to set the right `Content-Type`.
   */
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

    // All entries were notifications — JSON-RPC says nothing goes back.
    if (responses.length === 0) return new Uint8Array(0);
    return jsonBytes(isBatch ? responses : responses[0]!);
  }

  /** Dispatch one JSON-RPC entry; `undefined` means notification, no reply. */
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

    // Reuse the TOON-RPC dispatcher: same registry, same semantics; only the
    // envelope field differs, and it is rewritten on the way back out.
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

/**
 * Re-encode one already-parsed RPC message in the named dialect.
 *
 * The envelope field travels with the dialect: `jsonrpc: "2.0"` on the JSON
 * wire, `toonrpc: "1.0"` on the TOON wire. Everything else is preserved.
 */
export function encodeMessage(message: Record<string, JsonValue>, protocol: Protocol): string {
  const { jsonrpc: _j, toonrpc: _t, ...rest } = message as Record<string, JsonValue>;
  if (protocol === 'jsonrpc') {
    return JSON.stringify({ jsonrpc: JSONRPC_VERSION, ...rest });
  }
  return encode({ toonrpc: TOONRPC_VERSION, ...rest } as JsonValue);
}

/**
 * Read one already-framed message in either dialect into a plain object with a
 * `jsonrpc: "2.0"` envelope — the shape JSON-RPC consumers (e.g. an ACP
 * connection) already understand. Returns the message and the dialect it wore.
 */
export function decodeMessage(frame: string): { message: Record<string, JsonValue>; protocol: Protocol } {
  // A framed RPC message is always an object, so the resident-wire rule is
  // exact here: a frame opening with `{` is JSON, anything else is TOON. The
  // headier `detectProtocol` sniff exists for HTTP bodies, which may be
  // batches (`[`) and carry a content-type.
  const protocol: Protocol = frame.trimStart().startsWith('{') ? 'jsonrpc' : 'toonrpc';
  const raw = (protocol === 'jsonrpc' ? JSON.parse(frame) : decode(frame)) as Record<string, JsonValue>;
  const { jsonrpc: _j, toonrpc: _t, ...rest } = raw;
  return { message: { jsonrpc: JSONRPC_VERSION, ...rest }, protocol };
}

export type { Id, Params };
