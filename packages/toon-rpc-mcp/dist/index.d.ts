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
import type { JsonValue } from '@reddb-io/toon';
export declare const MCP_PROTOCOL_VERSION = "2025-06-18";
export type JsonObject = {
    [key: string]: JsonValue;
};
export type RequestId = string | number;
export interface Implementation {
    name: string;
    version: string;
    title?: string;
}
export interface Tool {
    name: string;
    title?: string;
    description?: string;
    inputSchema: JsonObject;
    outputSchema?: JsonObject;
    annotations?: JsonObject;
}
export interface TextContent {
    type: 'text';
    text: string;
}
export type ContentBlock = TextContent | ({
    type: string;
} & JsonObject);
export interface CallToolResult {
    content: ContentBlock[];
    structuredContent?: JsonObject;
    isError?: boolean;
}
export interface Resource {
    uri: string;
    name: string;
    title?: string;
    description?: string;
    mimeType?: string;
    size?: number;
}
export type ResourceContents = {
    uri: string;
    mimeType?: string;
    text: string;
} | {
    uri: string;
    mimeType?: string;
    blob: string;
};
export interface ReadResourceResult {
    contents: ResourceContents[];
}
export interface PromptArgument {
    name: string;
    title?: string;
    description?: string;
    required?: boolean;
}
export interface Prompt {
    name: string;
    title?: string;
    description?: string;
    arguments?: PromptArgument[];
}
export interface PromptMessage {
    role: 'user' | 'assistant';
    content: ContentBlock;
}
export interface GetPromptResult {
    description?: string;
    messages: PromptMessage[];
}
/**
 * What a server offers. A feature is advertised in the capabilities only
 * when its section is present.
 */
export interface McpService {
    serverInfo: Implementation;
    instructions?: string;
    tools?: {
        list(): Tool[] | Promise<Tool[]>;
        /** Throw `McpError.unknownTool(name)` for a tool that does not exist. */
        call(name: string, args: JsonObject): CallToolResult | Promise<CallToolResult>;
    };
    resources?: {
        list(): Resource[] | Promise<Resource[]>;
        /** Throw `McpError.resourceNotFound(uri)` for a resource that does not exist. */
        read(uri: string): ReadResourceResult | Promise<ReadResourceResult>;
    };
    prompts?: {
        list(): Prompt[] | Promise<Prompt[]>;
        /** Throw `McpError.invalidParams(...)` for an unknown prompt or missing argument. */
        get(name: string, args: Record<string, string>): GetPromptResult | Promise<GetPromptResult>;
    };
}
/** An error answered as a JSON-RPC error object. */
export declare class McpError extends Error {
    readonly code: number;
    readonly data?: JsonValue | undefined;
    constructor(code: number, message: string, data?: JsonValue | undefined);
    static invalidParams(message: string): McpError;
    static unknownTool(name: string): McpError;
    static resourceNotFound(uri: string): McpError;
}
/** One text content block. */
export declare function textContent(text: string): TextContent;
/** A value rendered as TOON inside a text content block. */
export declare function toonContent(value: JsonValue): TextContent;
export declare const CallToolResult: {
    text: (text: string) => CallToolResult;
    /** A tool-level failure the model should see, not a protocol error. */
    error: (message: string) => CallToolResult;
    /** `value` as TOON text, and as `structuredContent` when it is an object. */
    toon: (value: JsonValue) => CallToolResult;
};
type Outgoing = JsonObject;
/** One MCP session: create one per connection. */
export declare class McpServer {
    private readonly service;
    private initialized;
    constructor(service: McpService);
    /** Answer one line of newline-delimited JSON; undefined when there is none. */
    handleLine(line: string): Promise<string | undefined>;
    /** Answer one decoded message; undefined for a notification or response. */
    handleMessage(message: unknown): Promise<Outgoing | undefined>;
    private dispatch;
    private initialize;
}
export {};
//# sourceMappingURL=index.d.ts.map