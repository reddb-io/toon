import type { JsonValue } from '@reddb-io/toon';
import { Server } from '@reddb-io/toon-rpc';
export declare const MCP_PROTOCOL_VERSION = "2026-07-28";
export declare const MCP_NS = "io.modelcontextprotocol";
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
export interface Capability {
}
export interface ToolsCapability {
    listChanged?: boolean;
}
export interface ServerCapabilities {
    tools?: ToolsCapability;
    resources?: Capability;
    prompts?: Capability;
    [k: string]: unknown;
}
export interface Tool {
    name: string;
    title?: string;
    description?: string;
    inputSchema: JsonValue;
    annotations?: JsonValue;
    [k: string]: unknown;
}
export interface Resource {
    uri: string;
    name: string;
    title?: string;
    description?: string;
    mimeType?: string;
    [k: string]: unknown;
}
export interface Prompt {
    name: string;
    title?: string;
    description?: string;
    arguments?: Array<{
        name: string;
        description?: string;
        required?: boolean;
        [k: string]: unknown;
    }>;
    [k: string]: unknown;
}
export interface CallToolResponse {
    resultType: 'complete';
    content: Array<{
        type: string;
        [k: string]: JsonValue;
    }>;
    isError?: boolean;
}
export declare const CallToolResponse: {
    text: (text: string) => CallToolResponse;
    error: (message: string) => CallToolResponse;
};
export interface McpService {
    serverInfo(): ServerInfo;
    listTools(): Tool[];
    listResources(): Resource[];
    listPrompts(): Prompt[];
    readResource(uri: string): Promise<JsonValue> | JsonValue;
    getPrompt(name: string, args?: JsonValue): Promise<JsonValue> | JsonValue;
    callTool(name: string, args: JsonValue): CallToolResponse | Promise<CallToolResponse>;
}
export interface DiscoverResponse {
    resultType: 'complete';
    supportedVersions: string[];
    capabilities: ServerCapabilities;
    _meta?: Record<string, ServerInfo>;
    ttlMs?: number;
    cacheScope?: string;
}
export declare function createMcpDispatcher(service: McpService): Server;
export declare function encodeMcpMessage(value: JsonValue): string;
export declare function decodeMcpMessage(text: string): JsonValue;
//# sourceMappingURL=index.d.ts.map