/**
 * JSON-RPC 2.0 wire types for MCP.
 *
 * MCP is plain JSON-RPC 2.0. Per Spec #389 §9, TOON and TOON-RPC extensions are
 * never presented as MCP wire, so this module uses `JSON.parse`/`JSON.stringify`
 * only and shares no codec with `@reddb-io/toon-rpc`.
 */
/** Invalid JSON was received. */
export const PARSE_ERROR = -32700;
/** The JSON sent is not a valid Request object. */
export const INVALID_REQUEST = -32600;
/** The method does not exist or is not available. */
export const METHOD_NOT_FOUND = -32601;
/** Invalid method parameters. MCP also uses this for "resource not found". */
export const INVALID_PARAMS = -32602;
/** Internal JSON-RPC error. */
export const INTERNAL_ERROR = -32603;
/** `UnsupportedProtocolVersionError`, defined by MCP revision 2026-07-28. */
export const UNSUPPORTED_PROTOCOL_VERSION = -32022;
export function success(id, result) {
    return { jsonrpc: '2.0', result, id };
}
export function failure(id, error) {
    return { jsonrpc: '2.0', error, id };
}
export function rpcError(code, message, data) {
    return data === undefined ? { code, message } : { code, message, data };
}
/**
 * Serialize a response as one line.
 *
 * `JSON.stringify` escapes control characters inside strings, so the result
 * never contains a raw newline — the invariant the stdio framing depends on.
 */
export function toLine(response) {
    return JSON.stringify(response);
}
//# sourceMappingURL=jsonrpc.js.map