/** MCP method dispatch over JSON-RPC 2.0. */
import { INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR, UNSUPPORTED_PROTOCOL_VERSION, failure, rpcError, success, toLine, } from './jsonrpc.js';
import { FIELD_PROTOCOL_VERSION, FIELD_SERVER_INFO, MCP_LEGACY_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION, McpError, } from './types.js';
const ERROR_CODES = {
    method_not_found: METHOD_NOT_FOUND,
    // The spec assigns -32602 to a missing resource or prompt.
    tool_not_found: INVALID_PARAMS,
    resource_not_found: INVALID_PARAMS,
    prompt_not_found: INVALID_PARAMS,
    invalid_params: INVALID_PARAMS,
    internal: INTERNAL_ERROR,
};
export class McpDispatcher {
    #service;
    #legacyInitialize;
    constructor(service, options = {}) {
        this.#service = service;
        this.#legacyInitialize = options.legacyInitialize ?? false;
    }
    get supportedVersions() {
        return this.#legacyInitialize
            ? [MCP_PROTOCOL_VERSION, MCP_LEGACY_PROTOCOL_VERSION]
            : [MCP_PROTOCOL_VERSION];
    }
    /**
     * Handle one raw newline-delimited JSON line.
     *
     * Resolves to the response line to write, or `null` for a notification.
     */
    async handleLine(line) {
        const trimmed = line.trim();
        if (trimmed === '')
            return null;
        let parsed;
        try {
            parsed = JSON.parse(trimmed);
        }
        catch (e) {
            return toLine(failure(null, rpcError(PARSE_ERROR, `Parse error: ${e.message}`)));
        }
        const response = await this.handleMessage(parsed);
        return response === null ? null : toLine(response);
    }
    /** Handle one decoded message. Returns `null` for a notification. */
    async handleMessage(message) {
        if (typeof message !== 'object' || message === null || Array.isArray(message)) {
            // Batches are not part of MCP: every message is a single object.
            return failure(null, rpcError(INVALID_REQUEST, 'Invalid Request: expected a JSON object'));
        }
        const raw = message;
        // Only an absent `id` key denotes a notification; explicit null is a request.
        const isNotification = !('id' in raw);
        const id = (raw.id ?? null);
        if (raw.jsonrpc !== '2.0') {
            if (isNotification)
                return null;
            return failure(id, rpcError(INVALID_REQUEST, 'jsonrpc must be exactly "2.0"'));
        }
        if (typeof raw.method !== 'string') {
            if (isNotification)
                return null;
            return failure(id, rpcError(INVALID_REQUEST, 'Invalid Request: "method" must be a string'));
        }
        if (raw.params !== undefined && (typeof raw.params !== 'object' || raw.params === null)) {
            if (isNotification)
                return null;
            return failure(id, rpcError(INVALID_PARAMS, 'Invalid params: must be an object or array'));
        }
        const request = {
            jsonrpc: '2.0',
            method: raw.method,
            params: raw.params,
        };
        // Notifications are accepted and never answered, known or not.
        if (isNotification)
            return null;
        const versionError = this.#checkProtocolVersion(request);
        if (versionError)
            return failure(id, versionError);
        try {
            return success(id, await this.#route(request.method, request));
        }
        catch (e) {
            if (e instanceof McpError) {
                return failure(id, rpcError(ERROR_CODES[e.kind], e.message, e.data));
            }
            return failure(id, rpcError(INTERNAL_ERROR, `Internal error: ${e.message}`));
        }
    }
    /**
     * Reject a request whose declared `_meta` protocol version is not served.
     * Absent metadata is accepted, so the `server/discover` probe still works.
     */
    #checkProtocolVersion(request) {
        const params = request.params;
        const meta = params?._meta;
        const requested = meta?.[FIELD_PROTOCOL_VERSION];
        if (typeof requested !== 'string')
            return null;
        const supported = this.supportedVersions;
        if (supported.includes(requested))
            return null;
        return rpcError(UNSUPPORTED_PROTOCOL_VERSION, 'Unsupported protocol version', {
            supported,
            requested,
        });
    }
    async #capabilities() {
        if (this.#service.capabilities)
            return this.#service.capabilities();
        const caps = {};
        if ((await this.#listTools()).length > 0)
            caps.tools = { listChanged: false };
        if ((await this.#listResources()).length > 0)
            caps.resources = {};
        if ((await this.#listPrompts()).length > 0)
            caps.prompts = {};
        return caps;
    }
    async #listTools() {
        return (await this.#service.listTools?.()) ?? [];
    }
    async #listResources() {
        return (await this.#service.listResources?.()) ?? [];
    }
    async #listPrompts() {
        return (await this.#service.listPrompts?.()) ?? [];
    }
    async discover() {
        const instructions = this.#service.instructions?.();
        return omitUndefined({
            resultType: 'complete',
            supportedVersions: this.supportedVersions,
            capabilities: await this.#capabilities(),
            _meta: { [FIELD_SERVER_INFO]: this.#service.serverInfo() },
            instructions,
        });
    }
    async #route(method, request) {
        const params = (request.params ?? {});
        switch (method) {
            case 'ping':
                return {};
            case 'server/discover':
                return this.discover();
            case 'initialize':
                return this.#initialize();
            case 'tools/list':
                return omitUndefined({ resultType: 'complete', tools: await this.#listTools() });
            case 'tools/call':
                return this.#callTool(params);
            case 'resources/list':
                return omitUndefined({ resultType: 'complete', resources: await this.#listResources() });
            case 'resources/read':
                return this.#readResource(params);
            case 'prompts/list':
                return omitUndefined({ resultType: 'complete', prompts: await this.#listPrompts() });
            case 'prompts/get':
                return this.#getPrompt(params);
            default:
                throw new McpError('method_not_found', `Method not found: ${method}`);
        }
    }
    async #initialize() {
        if (!this.#legacyInitialize) {
            // A modern-only server names its versions in the error, because a legacy
            // client has no fall-forward mechanism and may show only this message.
            throw new McpError('method_not_found', `Method not found: initialize. This server speaks MCP ${MCP_PROTOCOL_VERSION}, which ` +
                `replaces the initialize handshake with per-request _meta. Call server/discover instead.`, { supported: this.supportedVersions });
        }
        return omitUndefined({
            protocolVersion: MCP_LEGACY_PROTOCOL_VERSION,
            capabilities: await this.#capabilities(),
            serverInfo: this.#service.serverInfo(),
            instructions: this.#service.instructions?.(),
        });
    }
    async #callTool(params) {
        const name = requireString(params, 'name');
        const args = (params.arguments ?? {});
        // An unknown tool is a protocol error, not a tool execution error.
        const tools = await this.#listTools();
        if (!tools.some((t) => t.name === name)) {
            throw new McpError('tool_not_found', `Unknown tool: ${name}`);
        }
        return omitUndefined(await this.#service.callTool(name, args));
    }
    async #readResource(params) {
        const uri = requireString(params, 'uri');
        const contents = (await this.#service.readResource?.(uri)) ?? [];
        // An empty contents array is ambiguous and must not stand in for a
        // missing resource.
        if (contents.length === 0)
            throw McpError.resourceNotFound(uri);
        return omitUndefined({ resultType: 'complete', contents });
    }
    async #getPrompt(params) {
        const name = requireString(params, 'name');
        if (!this.#service.getPrompt)
            throw McpError.promptNotFound(name);
        return omitUndefined(await this.#service.getPrompt(name, params.arguments));
    }
}
function requireString(params, key) {
    const value = params[key];
    if (typeof value !== 'string') {
        throw McpError.invalidParams(value === undefined
            ? `Invalid params: missing "${key}"`
            : `Invalid params: "${key}" must be a string`);
    }
    return value;
}
/**
 * Drop `undefined` properties so optional fields are omitted from the wire
 * rather than serialized as `null`, which the schema does not accept.
 */
function omitUndefined(value) {
    if (Array.isArray(value))
        return value.map(omitUndefined);
    if (value === null || typeof value !== 'object')
        return value;
    const out = {};
    for (const [k, v] of Object.entries(value)) {
        if (v !== undefined)
            out[k] = omitUndefined(v);
    }
    return out;
}
/** Convenience constructor. */
export function createMcpDispatcher(service, options) {
    return new McpDispatcher(service, options);
}
//# sourceMappingURL=dispatcher.js.map