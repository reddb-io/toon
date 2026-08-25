/**
 * MCP data types for the pinned protocol revision.
 *
 * Field names and result shapes follow the official schema for
 * {@link MCP_PROTOCOL_VERSION}; see `docs/mcp-conformance.md` for the
 * per-method citations.
 */
import type { JsonValue } from './jsonrpc.js';
/**
 * The MCP protocol revision this package implements.
 *
 * `2026-07-28` is the current revision: it replaces the `initialize` handshake
 * of `2025-11-25` and earlier with per-request `_meta` and a mandatory
 * `server/discover`.
 */
export declare const MCP_PROTOCOL_VERSION = "2026-07-28";
/** Legacy revision, served only when dual-era mode is enabled. */
export declare const MCP_LEGACY_PROTOCOL_VERSION = "2025-11-25";
/** Reserved namespace for MCP metadata fields. */
export declare const MCP_NS = "io.modelcontextprotocol";
export declare const FIELD_PROTOCOL_VERSION = "io.modelcontextprotocol/protocolVersion";
export declare const FIELD_CLIENT_INFO = "io.modelcontextprotocol/clientInfo";
export declare const FIELD_CLIENT_CAPABILITIES = "io.modelcontextprotocol/clientCapabilities";
export declare const FIELD_SERVER_INFO = "io.modelcontextprotocol/serverInfo";
export declare const FIELD_SUBSCRIPTION_ID = "io.modelcontextprotocol/subscriptionId";
/** Server identity. Self-reported and unverified; for display only. */
export interface ServerInfo {
    name: string;
    version: string;
    title?: string;
}
export interface ClientInfo {
    name: string;
    version: string;
    title?: string;
}
export interface ToolsCapability {
    listChanged?: boolean;
}
export interface ResourcesCapability {
    listChanged?: boolean;
    subscribe?: boolean;
}
export interface PromptsCapability {
    listChanged?: boolean;
}
export interface ServerCapabilities {
    tools?: ToolsCapability;
    resources?: ResourcesCapability;
    prompts?: PromptsCapability;
    [k: string]: unknown;
}
export interface Tool {
    name: string;
    title?: string;
    description?: string;
    /** MUST be a valid JSON Schema object, never null. */
    inputSchema: JsonValue;
    outputSchema?: JsonValue;
    annotations?: JsonValue;
}
export interface Resource {
    uri: string;
    name: string;
    title?: string;
    description?: string;
    /** Spelled `mimeType`; the schema defines no snake_case alias. */
    mimeType?: string;
    size?: number;
}
/** One entry of a `resources/read` result. */
export interface ResourceContents {
    uri: string;
    mimeType?: string;
    text?: string;
    /** Base64-encoded binary payload. */
    blob?: string;
}
export interface PromptArgument {
    name: string;
    description?: string;
    required?: boolean;
}
export interface Prompt {
    name: string;
    title?: string;
    description?: string;
    arguments?: PromptArgument[];
}
export type Content = {
    type: 'text';
    text: string;
} | {
    type: 'image';
    data: string;
    mimeType: string;
} | {
    type: 'audio';
    data: string;
    mimeType: string;
} | {
    type: 'resource_link';
    uri: string;
    name: string;
    description?: string;
    mimeType?: string;
} | {
    type: 'resource';
    resource: JsonValue;
};
export interface PromptMessage {
    role: 'user' | 'assistant';
    content: Content;
}
/** Result of `tools/call`. */
export interface CallToolResult {
    resultType: 'complete';
    content: Content[];
    structuredContent?: JsonValue;
    isError?: boolean;
}
export declare const CallToolResult: {
    text(text: string): CallToolResult;
    /**
     * A *tool execution* error: a normal result with `isError: true`, which the
     * model can read and self-correct from. Protocol-level failures raise a
     * JSON-RPC error instead.
     */
    error(message: string): CallToolResult;
};
export interface DiscoverResult {
    resultType: 'complete';
    supportedVersions: string[];
    capabilities: ServerCapabilities;
    _meta?: {
        [FIELD_SERVER_INFO]?: ServerInfo;
    };
    instructions?: string;
    ttlMs?: number;
    cacheScope?: string;
}
export interface ListToolsResult {
    resultType: 'complete';
    tools: Tool[];
    nextCursor?: string;
}
export interface ListResourcesResult {
    resultType: 'complete';
    resources: Resource[];
    nextCursor?: string;
}
export interface ReadResourceResult {
    resultType: 'complete';
    contents: ResourceContents[];
}
export interface ListPromptsResult {
    resultType: 'complete';
    prompts: Prompt[];
    nextCursor?: string;
}
export interface GetPromptResult {
    resultType: 'complete';
    description?: string;
    messages: PromptMessage[];
}
export interface InitializeResult {
    protocolVersion: string;
    capabilities: ServerCapabilities;
    serverInfo: ServerInfo;
    instructions?: string;
}
/** Failure a service reports; the dispatcher maps each to a JSON-RPC code. */
export declare class McpError extends Error {
    readonly kind: 'method_not_found' | 'tool_not_found' | 'resource_not_found' | 'prompt_not_found' | 'invalid_params' | 'internal';
    readonly data?: JsonValue | undefined;
    constructor(kind: 'method_not_found' | 'tool_not_found' | 'resource_not_found' | 'prompt_not_found' | 'invalid_params' | 'internal', message: string, data?: JsonValue | undefined);
    static resourceNotFound(uri: string): McpError;
    static promptNotFound(name: string): McpError;
    static invalidParams(message: string): McpError;
}
/**
 * A Model Context Protocol server.
 *
 * Every method may be synchronous or return a promise.
 */
export interface McpService {
    serverInfo(): ServerInfo;
    instructions?(): string | undefined;
    capabilities?(): ServerCapabilities;
    listTools?(): Tool[] | Promise<Tool[]>;
    listResources?(): Resource[] | Promise<Resource[]>;
    listPrompts?(): Prompt[] | Promise<Prompt[]>;
    /** Returning an empty array is treated as "not found". */
    readResource?(uri: string): ResourceContents[] | Promise<ResourceContents[]>;
    getPrompt?(name: string, args?: JsonValue): GetPromptResult | Promise<GetPromptResult>;
    callTool(name: string, args: JsonValue): CallToolResult | Promise<CallToolResult>;
}
//# sourceMappingURL=types.d.ts.map