/**
 * A Model Context Protocol server for MCP revision `2026-07-28`.
 *
 * # Protocol pin
 *
 * This package targets exactly one revision, {@link MCP_PROTOCOL_VERSION}. That
 * revision carries the protocol version, client identity, and client
 * capabilities as per-request `_meta` and requires servers to implement
 * `server/discover`; it replaces the `initialize` handshake used by
 * `2025-11-25` and earlier. A dual-era server that also answers `initialize` is
 * available through the `legacyInitialize` dispatcher option.
 *
 * # Wire format
 *
 * MCP is plain JSON-RPC 2.0. Per Spec #389 §9, TOON and TOON-RPC extensions are
 * never presented as MCP wire, so this package shares no codec with
 * `@reddb-io/toon-rpc`.
 *
 * # Transports
 *
 * Only stdio is implemented, framed as one JSON-RPC message per line. There is
 * no HTTP transport in this package; the Rust crate
 * `reddb-io-toon-rpc-mcp` provides a POST-only HTTP endpoint.
 *
 * @example
 * ```ts
 * import { createMcpDispatcher, serveStdio, CallToolResult } from '@reddb-io/toon-rpc-mcp';
 *
 * const dispatcher = createMcpDispatcher({
 *   serverInfo: () => ({ name: 'echo', version: '1.0.0' }),
 *   listTools: () => [{
 *     name: 'echo',
 *     inputSchema: { type: 'object', properties: { text: { type: 'string' } }, required: ['text'] },
 *   }],
 *   callTool: (_name, args) => CallToolResult.text(String((args as any).text)),
 * });
 *
 * serveStdio(dispatcher);
 * ```
 */
export { McpDispatcher, createMcpDispatcher } from './dispatcher.js';
export { serveStdio, serveStdioWith } from './stdio.js';
export { CallToolResult, McpError, MCP_LEGACY_PROTOCOL_VERSION, MCP_NS, MCP_PROTOCOL_VERSION, FIELD_CLIENT_CAPABILITIES, FIELD_CLIENT_INFO, FIELD_PROTOCOL_VERSION, FIELD_SERVER_INFO, FIELD_SUBSCRIPTION_ID, } from './types.js';
export { INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR, UNSUPPORTED_PROTOCOL_VERSION, } from './jsonrpc.js';
//# sourceMappingURL=index.js.map