/**
 * Multi-protocol RPC: auto-detects JSON-RPC 2.0 vs TOON-RPC 1.0 on the wire
 * and answers in the same format the client used.
 */
import { decode, encode } from '@reddb-io/toon';
import { TOONRPC_VERSION, snapshotCoreValue } from '@reddb-io/toon-rpc';
export { Server } from '@reddb-io/toon-rpc';
export const JSONRPC_VERSION = '2.0';
/** MIME type for HTTP `Content-Type` / `Accept` negotiation. */
export function contentTypeFor(protocol) {
    return protocol === 'jsonrpc' ? 'application/json' : 'application/toon';
}
/** Detect the protocol from a content-type hint and/or raw bytes. */
export function detectProtocol(raw, contentType) {
    return detect(raw, contentType).protocol;
}
function detect(raw, contentType) {
    if (contentType !== undefined) {
        const mediaType = contentType.split(';', 1)[0].trim().toLowerCase();
        if (mediaType === 'application/json')
            return { protocol: 'jsonrpc' };
        if (mediaType === 'application/toon')
            return { protocol: 'toonrpc' };
    }
    let text;
    try {
        text =
            typeof raw === 'string'
                ? raw
                : new TextDecoder('utf-8', { fatal: true }).decode(raw);
    }
    catch {
        return { protocol: 'toonrpc' };
    }
    const trimmed = text.trimStart();
    if (!trimmed.startsWith('{') && !trimmed.startsWith('['))
        return { protocol: 'toonrpc' };
    try {
        const value = JSON.parse(trimmed);
        if (hasJsonRpcMember(value)) {
            return {
                protocol: 'jsonrpc',
                cachedJson: { value },
            };
        }
    }
    catch {
        // Existing fallback policy treats unrecognized input as TOON-RPC.
    }
    return { protocol: 'toonrpc' };
}
/** A single method registry behind JSON-RPC and TOON-RPC wire formats. */
export class MultiRpc {
    server;
    constructor(server) {
        this.server = server;
    }
    async handle(raw, contentType) {
        const { body } = await this.handleWithProtocol(raw, contentType);
        return body;
    }
    async handleWithProtocol(raw, contentType) {
        const { protocol, cachedJson } = detect(raw, contentType);
        const body = protocol === 'jsonrpc'
            ? await this.handleJsonRpc(raw, cachedJson)
            : typeof raw === 'string'
                ? await this.server.handleText(raw)
                : await this.server.handle(raw);
        return { protocol, body };
    }
    async handleJsonRpc(raw, cached) {
        let value;
        try {
            if (cached) {
                value = cached.value;
            }
            else {
                const text = typeof raw === 'string'
                    ? raw
                    : new TextDecoder('utf-8', { fatal: true }).decode(raw);
                value = JSON.parse(text);
            }
        }
        catch (err) {
            const message = err instanceof Error ? err.message : 'Parse error';
            return jsonBytes({
                jsonrpc: JSONRPC_VERSION,
                error: { code: -32700, message: `Parse error: ${message}` },
                id: null,
            });
        }
        const isBatch = Array.isArray(value);
        const entries = Array.isArray(value) ? value : [value];
        if (entries.length === 0) {
            return jsonBytes({
                jsonrpc: JSONRPC_VERSION,
                error: { code: -32600, message: 'Invalid Request: empty batch' },
                id: null,
            });
        }
        const responses = [];
        for (const entry of entries) {
            const response = await this.dispatchJsonRpcEntry(entry);
            if (response !== undefined)
                responses.push(response);
        }
        if (responses.length === 0)
            return new Uint8Array(0);
        return textBytes(isBatch
            ? `[${responses.map(({ encoded }) => encoded).join(',')}]`
            : responses[0].encoded);
    }
    async dispatchJsonRpcEntry(entry) {
        const snapshot = snapshotCoreValue(entry);
        const dispatched = await this.server.dispatchEntry(snapshot === undefined ? undefined : toToonEntry(snapshot));
        if (dispatched === undefined)
            return undefined;
        return preflightJsonResponse(dispatched);
    }
}
function hasJsonRpcMember(value) {
    if (Array.isArray(value))
        return value.some(hasOwnJsonRpcMember);
    return hasOwnJsonRpcMember(value);
}
function hasOwnJsonRpcMember(value) {
    return (value !== null &&
        typeof value === 'object' &&
        !Array.isArray(value) &&
        Object.prototype.hasOwnProperty.call(value, 'jsonrpc'));
}
function toToonEntry(entry) {
    if (entry === null || typeof entry !== 'object' || Array.isArray(entry))
        return entry;
    const record = entry;
    const members = [];
    if (Object.prototype.hasOwnProperty.call(record, 'jsonrpc') &&
        record.jsonrpc === JSONRPC_VERSION) {
        members.push(['toonrpc', TOONRPC_VERSION]);
    }
    for (const [key, value] of Object.entries(record)) {
        if (key !== 'jsonrpc' && key !== 'toonrpc')
            members.push([key, value]);
    }
    return Object.fromEntries(members);
}
function toJsonResponse(response) {
    const members = Object.entries(response).filter(([key]) => key !== 'toonrpc');
    return Object.fromEntries([['jsonrpc', JSONRPC_VERSION], ...members]);
}
function preflightJsonResponse(response) {
    const value = toJsonResponse(response);
    try {
        return { value, encoded: stringifyJson(value) };
    }
    catch {
        const fallback = toJsonResponse({
            toonrpc: TOONRPC_VERSION,
            error: { code: -32603, message: 'Internal error' },
            id: response.id,
        });
        return { value: fallback, encoded: stringifyJson(fallback) };
    }
}
function jsonBytes(value) {
    return textBytes(stringifyJson(value));
}
function textBytes(value) {
    return new TextEncoder().encode(value);
}
function stringifyJson(value) {
    const encoded = JSON.stringify(value);
    if (encoded === undefined)
        throw new TypeError('JSON value is not encodable');
    return encoded;
}
/** Re-encode an already-parsed RPC message or batch in the named dialect. */
export function encodeMessage(message, protocol) {
    const snapshot = snapshotMessageDocument(message);
    const translated = isMessageBatch(snapshot)
        ? snapshot.map((entry) => translateMessage(entry, protocol))
        : translateMessage(snapshot, protocol);
    return protocol === 'jsonrpc'
        ? JSON.stringify(translated)
        : encode(translated);
}
/** Decode a framed message into the JSON-RPC-compatible object shape. */
export function decodeMessage(frame) {
    const { protocol, cachedJson } = detect(frame);
    const raw = protocol === 'jsonrpc' ? cachedJson.value : decode(frame);
    const snapshot = snapshotMessageDocument(raw);
    const message = isMessageBatch(snapshot)
        ? snapshot.map((entry) => translateMessage(entry, 'jsonrpc'))
        : translateMessage(snapshot, 'jsonrpc');
    return { message, protocol };
}
function snapshotMessageDocument(value) {
    const snapshot = snapshotCoreValue(value);
    if (snapshot === undefined)
        throw new TypeError('RPC message is not a core value');
    const entries = Array.isArray(snapshot) ? snapshot : [snapshot];
    if (entries.some((entry) => entry === null || typeof entry !== 'object' || Array.isArray(entry))) {
        throw new TypeError('RPC message entries must be objects');
    }
    return (Array.isArray(snapshot) ? entries : entries[0]);
}
function isMessageBatch(message) {
    return Array.isArray(message);
}
function translateMessage(message, protocol) {
    const members = [
        [
            protocol === 'jsonrpc' ? 'jsonrpc' : 'toonrpc',
            protocol === 'jsonrpc' ? JSONRPC_VERSION : TOONRPC_VERSION,
        ],
    ];
    for (const [key, value] of Object.entries(message)) {
        if (key !== 'jsonrpc' && key !== 'toonrpc')
            members.push([key, value]);
    }
    return Object.fromEntries(members);
}
//# sourceMappingURL=index.js.map