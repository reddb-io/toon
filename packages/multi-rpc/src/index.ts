/**
 * Multi-protocol RPC: auto-detects JSON-RPC 2.0 vs TOON-RPC 1.0 on the wire
 * and answers in the same format the client used.
 */

import { decode, encode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';
import { Server, TOONRPC_VERSION, snapshotCoreValue } from '@reddb-io/toon-rpc';
import type { CoreObject, CoreValue, Id, Params, Response } from '@reddb-io/toon-rpc';

export { Server } from '@reddb-io/toon-rpc';

export const JSONRPC_VERSION = '2.0';

/** Wire protocol variants the dispatcher can negotiate. */
export type Protocol = 'jsonrpc' | 'toonrpc';
export type Message = CoreObject;
export type MessageDocument = Message | readonly Message[];

/** MIME type for HTTP `Content-Type` / `Accept` negotiation. */
export function contentTypeFor(protocol: Protocol): string {
  return protocol === 'jsonrpc' ? 'application/json' : 'application/toon';
}

/** Detect the protocol from a content-type hint and/or raw bytes. */
export function detectProtocol(raw: Uint8Array | string, contentType?: string): Protocol {
  return detect(raw, contentType).protocol;
}

function detect(
  raw: Uint8Array | string,
  contentType?: string
): { protocol: Protocol; cachedJson?: { value: unknown } } {
  if (contentType !== undefined) {
    const mediaType = contentType.split(';', 1)[0]!.trim().toLowerCase();
    if (mediaType === 'application/json') return { protocol: 'jsonrpc' };
    if (mediaType === 'application/toon') return { protocol: 'toonrpc' };
  }

  let text: string;
  try {
    text =
      typeof raw === 'string'
        ? raw
        : new TextDecoder('utf-8', { fatal: true }).decode(raw);
  } catch {
    return { protocol: 'toonrpc' };
  }
  const trimmed = text.trimStart();

  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return { protocol: 'toonrpc' };
  try {
    const value: unknown = JSON.parse(trimmed);
    if (hasJsonRpcMember(value)) {
      return {
        protocol: 'jsonrpc',
        cachedJson: { value },
      };
    }
  } catch {
    // Existing fallback policy treats unrecognized input as TOON-RPC.
  }

  return { protocol: 'toonrpc' };
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
    const { protocol, cachedJson } = detect(raw, contentType);
    const body =
      protocol === 'jsonrpc'
        ? await this.handleJsonRpc(raw, cachedJson)
        : typeof raw === 'string'
          ? await this.server.handleText(raw)
          : await this.server.handle(raw);
    return { protocol, body };
  }

  private async handleJsonRpc(
    raw: Uint8Array | string,
    cached?: { value: unknown }
  ): Promise<Uint8Array> {
    let value: unknown;
    try {
      if (cached) {
        value = cached.value;
      } else {
        const text =
          typeof raw === 'string'
            ? raw
            : new TextDecoder('utf-8', { fatal: true }).decode(raw);
        value = JSON.parse(text);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Parse error';
      return jsonBytes({
        jsonrpc: JSONRPC_VERSION,
        error: { code: -32700, message: `Parse error: ${message}` },
        id: null,
      });
    }

    const isBatch = Array.isArray(value);
    const entries: unknown[] = Array.isArray(value) ? value : [value];
    if (entries.length === 0) {
      return jsonBytes({
        jsonrpc: JSONRPC_VERSION,
        error: { code: -32600, message: 'Invalid Request: empty batch' },
        id: null,
      });
    }

    const responses: Array<{ value: JsonValue; encoded: string }> = [];
    for (const entry of entries) {
      const response = await this.dispatchJsonRpcEntry(entry);
      if (response !== undefined) responses.push(response);
    }

    if (responses.length === 0) return new Uint8Array(0);
    return textBytes(
      isBatch
        ? `[${responses.map(({ encoded }) => encoded).join(',')}]`
        : responses[0]!.encoded
    );
  }

  private async dispatchJsonRpcEntry(
    entry: unknown
  ): Promise<{ value: JsonValue; encoded: string } | undefined> {
    const snapshot = snapshotCoreValue(entry);
    const dispatched = await this.server.dispatchEntry(
      snapshot === undefined ? undefined : toToonEntry(snapshot)
    );
    if (dispatched === undefined) return undefined;

    return preflightJsonResponse(dispatched);
  }
}

function hasJsonRpcMember(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(hasOwnJsonRpcMember);
  return hasOwnJsonRpcMember(value);
}

function hasOwnJsonRpcMember(value: unknown): boolean {
  return (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    Object.prototype.hasOwnProperty.call(value, 'jsonrpc')
  );
}

function toToonEntry(entry: CoreValue): CoreValue {
  if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) return entry;
  const record = entry as CoreObject;
  const members: Array<[string, CoreValue]> = [];
  if (
    Object.prototype.hasOwnProperty.call(record, 'jsonrpc') &&
    record.jsonrpc === JSONRPC_VERSION
  ) {
    members.push(['toonrpc', TOONRPC_VERSION]);
  }
  for (const [key, value] of Object.entries(record)) {
    if (key !== 'jsonrpc' && key !== 'toonrpc') members.push([key, value]);
  }
  return Object.fromEntries(members) as CoreObject;
}

function toJsonResponse(response: Response): JsonValue {
  const members = Object.entries(response).filter(([key]) => key !== 'toonrpc');
  return Object.fromEntries([['jsonrpc', JSONRPC_VERSION], ...members]) as JsonValue;
}

function preflightJsonResponse(response: Response): { value: JsonValue; encoded: string } {
  const value = toJsonResponse(response);
  try {
    return { value, encoded: stringifyJson(value) };
  } catch {
    const fallback = toJsonResponse({
      toonrpc: TOONRPC_VERSION,
      error: { code: -32603, message: 'Internal error' },
      id: response.id,
    });
    return { value: fallback, encoded: stringifyJson(fallback) };
  }
}

function jsonBytes(value: unknown): Uint8Array {
  return textBytes(stringifyJson(value));
}

function textBytes(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

function stringifyJson(value: unknown): string {
  const encoded = JSON.stringify(value);
  if (encoded === undefined) throw new TypeError('JSON value is not encodable');
  return encoded;
}

/** Re-encode an already-parsed RPC message or batch in the named dialect. */
export function encodeMessage(message: MessageDocument, protocol: Protocol): string {
  const snapshot = snapshotMessageDocument(message);
  const translated = isMessageBatch(snapshot)
    ? snapshot.map((entry) => translateMessage(entry, protocol))
    : translateMessage(snapshot, protocol);
  return protocol === 'jsonrpc'
    ? JSON.stringify(translated)
    : encode(translated as unknown as JsonValue);
}

/** Decode a framed message into the JSON-RPC-compatible object shape. */
export function decodeMessage(frame: string): {
  message: MessageDocument;
  protocol: Protocol;
} {
  const { protocol, cachedJson } = detect(frame);
  const raw: unknown = protocol === 'jsonrpc' ? cachedJson!.value : decode(frame);
  const snapshot = snapshotMessageDocument(raw);
  const message = isMessageBatch(snapshot)
    ? snapshot.map((entry) => translateMessage(entry, 'jsonrpc'))
    : translateMessage(snapshot, 'jsonrpc');
  return { message, protocol };
}

function snapshotMessageDocument(value: unknown): MessageDocument {
  const snapshot = snapshotCoreValue(value);
  if (snapshot === undefined) throw new TypeError('RPC message is not a core value');
  const entries = Array.isArray(snapshot) ? snapshot : [snapshot];
  if (
    entries.some(
      (entry) => entry === null || typeof entry !== 'object' || Array.isArray(entry)
    )
  ) {
    throw new TypeError('RPC message entries must be objects');
  }
  return (Array.isArray(snapshot) ? entries : entries[0]!) as MessageDocument;
}

function isMessageBatch(message: MessageDocument): message is readonly Message[] {
  return Array.isArray(message);
}

function translateMessage(message: Message, protocol: Protocol): Message {
  const members: Array<[string, CoreValue]> = [
    [
      protocol === 'jsonrpc' ? 'jsonrpc' : 'toonrpc',
      protocol === 'jsonrpc' ? JSONRPC_VERSION : TOONRPC_VERSION,
    ],
  ];
  for (const [key, value] of Object.entries(message)) {
    if (key !== 'jsonrpc' && key !== 'toonrpc') members.push([key, value]);
  }
  return Object.fromEntries(members) as Message;
}

export type { Id, Params };
