/**
 * A Model Context Protocol server, pinned to the official 2025-06-18 schema.
 *
 * MCP is JSON-RPC 2.0, not TOON-RPC: messages are JSON objects, IDs are
 * strings or numbers (never null), params are objects, and this revision has
 * no batching. The server implements the lifecycle (`initialize`, then the
 * `notifications/initialized` notification; only `initialize` and `ping` are
 * answered before `initialize`, and every feature request after it) and the
 * tools, resources and prompts features for whichever of them the service
 * provides. TOON appears only as an optional encoding of
 * text content (`toonContent`), which a model reads like any other text.
 *
 * `handleMessage` answers one decoded message; `handleLine` answers one line
 * of the newline-delimited stdio transport (see `./stdio`).
 */
import { encode } from '@reddb-io/toon';
export const MCP_PROTOCOL_VERSION = '2025-06-18';
/** An error answered as a JSON-RPC error object. */
export class McpError extends Error {
    code;
    data;
    constructor(code, message, data) {
        super(message);
        this.code = code;
        this.data = data;
        this.name = 'McpError';
    }
    static invalidParams(message) {
        return new McpError(-32602, message);
    }
    static unknownTool(name) {
        return new McpError(-32602, `Unknown tool: ${name}`);
    }
    static resourceNotFound(uri) {
        return new McpError(-32002, 'Resource not found', { uri });
    }
}
/** One text content block. */
export function textContent(text) {
    return { type: 'text', text };
}
/** A value rendered as TOON inside a text content block. */
export function toonContent(value) {
    return textContent(encode(value));
}
export const CallToolResult = {
    text: (text) => ({ content: [textContent(text)] }),
    /** A tool-level failure the model should see, not a protocol error. */
    error: (message) => ({ content: [textContent(message)], isError: true }),
    /** `value` as TOON text, and as `structuredContent` when it is an object. */
    toon: (value) => ({
        content: [toonContent(value)],
        ...(isObject(value) ? { structuredContent: value } : {}),
    }),
};
const PARSE_ERROR = -32700;
const INVALID_REQUEST = -32600;
const METHOD_NOT_FOUND = -32601;
const INVALID_PARAMS = -32602;
const INTERNAL_ERROR = -32603;
/** One MCP session: create one per connection. */
export class McpServer {
    service;
    initialized = false;
    constructor(service) {
        this.service = service;
    }
    /** Answer one line of newline-delimited JSON; undefined when there is none. */
    async handleLine(line) {
        let message;
        try {
            message = JSON.parse(line);
        }
        catch {
            return JSON.stringify(errorResponse(null, PARSE_ERROR, 'Parse error'));
        }
        const answer = await this.handleMessage(message);
        return answer === undefined ? undefined : JSON.stringify(answer);
    }
    /** Answer one decoded message; undefined for a notification or response. */
    async handleMessage(message) {
        if (!isObject(message) || message.jsonrpc !== '2.0') {
            return errorResponse(null, INVALID_REQUEST, 'Invalid Request');
        }
        // A response to a request this server never sends is ignored.
        if (!('method' in message) && ('result' in message || 'error' in message))
            return undefined;
        const { method, params } = message;
        const hasId = 'id' in message;
        const id = message.id;
        if (typeof method !== 'string' ||
            (hasId && typeof id !== 'string' && typeof id !== 'number') ||
            (params !== undefined && !isObject(params))) {
            return errorResponse(null, INVALID_REQUEST, 'Invalid Request');
        }
        // Notifications (`notifications/initialized`, `notifications/cancelled`)
        // need no answer, and none changes what this server does.
        if (!hasId)
            return undefined;
        try {
            const result = await this.dispatch(method, (params ?? {}));
            return { jsonrpc: '2.0', id: id, result };
        }
        catch (error) {
            if (error instanceof McpError) {
                return errorResponse(id, error.code, error.message, error.data);
            }
            return errorResponse(id, INTERNAL_ERROR, 'Internal error');
        }
    }
    async dispatch(method, params) {
        if (method === 'ping')
            return {};
        if (method === 'initialize')
            return this.initialize();
        if (!this.initialized)
            throw new McpError(INVALID_REQUEST, 'Server not initialized');
        const { tools, resources, prompts } = this.service;
        switch (method) {
            case 'tools/list':
                if (tools)
                    return { tools: (await tools.list()) };
                break;
            case 'tools/call':
                if (tools) {
                    const name = requireString(params, 'name');
                    const args = params.arguments ?? {};
                    if (!isObject(args))
                        throw McpError.invalidParams('arguments must be an object');
                    return (await tools.call(name, args));
                }
                break;
            case 'resources/list':
                if (resources)
                    return { resources: (await resources.list()) };
                break;
            case 'resources/read':
                if (resources) {
                    return (await resources.read(requireString(params, 'uri')));
                }
                break;
            case 'prompts/list':
                if (prompts)
                    return { prompts: (await prompts.list()) };
                break;
            case 'prompts/get':
                if (prompts) {
                    const name = requireString(params, 'name');
                    const args = params.arguments ?? {};
                    if (!isObject(args) || !Object.values(args).every((value) => typeof value === 'string')) {
                        throw McpError.invalidParams('arguments must map names to strings');
                    }
                    return (await prompts.get(name, args));
                }
                break;
        }
        throw new McpError(METHOD_NOT_FOUND, 'Method not found');
    }
    initialize() {
        this.initialized = true;
        const { tools, resources, prompts, serverInfo, instructions } = this.service;
        const capabilities = {};
        if (tools)
            capabilities.tools = { listChanged: false };
        if (resources)
            capabilities.resources = { subscribe: false, listChanged: false };
        if (prompts)
            capabilities.prompts = { listChanged: false };
        // This server speaks one revision; a client asking for another decides
        // from this answer whether it can continue.
        return {
            protocolVersion: MCP_PROTOCOL_VERSION,
            capabilities,
            serverInfo: serverInfo,
            ...(instructions === undefined ? {} : { instructions }),
        };
    }
}
function requireString(params, key) {
    const value = params[key];
    if (typeof value !== 'string')
        throw McpError.invalidParams(`${key} must be a string`);
    return value;
}
function errorResponse(id, code, message, data) {
    return {
        jsonrpc: '2.0',
        id,
        error: { code, message, ...(data === undefined ? {} : { data }) },
    };
}
function isObject(value) {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
}
//# sourceMappingURL=index.js.map