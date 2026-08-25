/**
 * MCP data types for the pinned protocol revision.
 *
 * Field names and result shapes follow the official schema for
 * {@link MCP_PROTOCOL_VERSION}; see `docs/mcp-conformance.md` for the
 * per-method citations.
 */
/**
 * The MCP protocol revision this package implements.
 *
 * `2026-07-28` is the current revision: it replaces the `initialize` handshake
 * of `2025-11-25` and earlier with per-request `_meta` and a mandatory
 * `server/discover`.
 */
export const MCP_PROTOCOL_VERSION = '2026-07-28';
/** Legacy revision, served only when dual-era mode is enabled. */
export const MCP_LEGACY_PROTOCOL_VERSION = '2025-11-25';
/** Reserved namespace for MCP metadata fields. */
export const MCP_NS = 'io.modelcontextprotocol';
export const FIELD_PROTOCOL_VERSION = 'io.modelcontextprotocol/protocolVersion';
export const FIELD_CLIENT_INFO = 'io.modelcontextprotocol/clientInfo';
export const FIELD_CLIENT_CAPABILITIES = 'io.modelcontextprotocol/clientCapabilities';
export const FIELD_SERVER_INFO = 'io.modelcontextprotocol/serverInfo';
export const FIELD_SUBSCRIPTION_ID = 'io.modelcontextprotocol/subscriptionId';
export const CallToolResult = {
    text(text) {
        return { resultType: 'complete', content: [{ type: 'text', text }] };
    },
    /**
     * A *tool execution* error: a normal result with `isError: true`, which the
     * model can read and self-correct from. Protocol-level failures raise a
     * JSON-RPC error instead.
     */
    error(message) {
        return { resultType: 'complete', content: [{ type: 'text', text: message }], isError: true };
    },
};
/** Failure a service reports; the dispatcher maps each to a JSON-RPC code. */
export class McpError extends Error {
    kind;
    data;
    constructor(kind, message, data) {
        super(message);
        this.kind = kind;
        this.data = data;
        this.name = 'McpError';
    }
    static resourceNotFound(uri) {
        return new McpError('resource_not_found', 'Resource not found', { uri });
    }
    static promptNotFound(name) {
        return new McpError('prompt_not_found', 'Prompt not found', { name });
    }
    static invalidParams(message) {
        return new McpError('invalid_params', message);
    }
}
//# sourceMappingURL=types.js.map