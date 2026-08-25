/**
 * JSON-RPC 2.0 wire types for MCP.
 *
 * MCP is plain JSON-RPC 2.0. Per Spec #389 §9, TOON and TOON-RPC extensions are
 * never presented as MCP wire, so this module uses `JSON.parse`/`JSON.stringify`
 * only and shares no codec with `@reddb-io/toon-rpc`.
 */

export type JsonValue = null | boolean | number | string | JsonValue[] | { [k: string]: JsonValue };

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

export interface JsonRpcRequest {
  jsonrpc: string;
  method: string;
  params?: JsonValue;
  /** Absent means notification. An explicit `null` is still a request. */
  id?: JsonValue;
}

export interface JsonRpcError {
  code: number;
  message: string;
  data?: JsonValue;
}

export interface JsonRpcResponse {
  jsonrpc: '2.0';
  result?: JsonValue;
  error?: JsonRpcError;
  id: JsonValue;
}

export function success(id: JsonValue, result: JsonValue): JsonRpcResponse {
  return { jsonrpc: '2.0', result, id };
}

export function failure(id: JsonValue, error: JsonRpcError): JsonRpcResponse {
  return { jsonrpc: '2.0', error, id };
}

export function rpcError(code: number, message: string, data?: JsonValue): JsonRpcError {
  return data === undefined ? { code, message } : { code, message, data };
}

/**
 * Serialize a response as one line.
 *
 * `JSON.stringify` escapes control characters inside strings, so the result
 * never contains a raw newline — the invariant the stdio framing depends on.
 */
export function toLine(response: JsonRpcResponse): string {
  return JSON.stringify(response);
}
