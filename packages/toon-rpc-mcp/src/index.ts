/**
 * A Model Context Protocol server, pinned to the official 2025-06-18 schema.
 *
 * MCP is JSON-RPC 2.0, not TOON-RPC: messages are JSON objects, IDs are
 * strings or numbers (never null), params are objects, and this revision has
 * no batching. The server implements the lifecycle: before `initialize` only
 * `ping` is answered, and feature requests are accepted once `initialize` is
 * (the client's `notifications/initialized` needs no answer). It serves the
 * tools, resources and prompts features for whichever of them the service
 * provides. TOON appears only as an optional encoding of text content
 * (`toonContent`), which a model reads like any other text.
 *
 * `handleMessage` answers one decoded message; `handleLine` answers one line
 * of the newline-delimited stdio transport (see `./stdio`).
 */

import { encode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';

export const MCP_PROTOCOL_VERSION = '2025-06-18';

export type JsonObject = { [key: string]: JsonValue };
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

export type ContentBlock = TextContent | ({ type: string } & JsonObject);

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

export type ResourceContents =
  | { uri: string; mimeType?: string; text: string }
  | { uri: string; mimeType?: string; blob: string };

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
export class McpError extends Error {
  constructor(
    readonly code: number,
    message: string,
    readonly data?: JsonValue
  ) {
    super(message);
    this.name = 'McpError';
  }

  static invalidParams(message: string): McpError {
    return new McpError(-32602, message);
  }

  static unknownTool(name: string): McpError {
    return new McpError(-32602, `Unknown tool: ${name}`);
  }

  static resourceNotFound(uri: string): McpError {
    return new McpError(-32002, 'Resource not found', { uri });
  }
}

/** One text content block. */
export function textContent(text: string): TextContent {
  return { type: 'text', text };
}

/** A value rendered as TOON inside a text content block. */
export function toonContent(value: JsonValue): TextContent {
  return textContent(encode(value));
}

export const CallToolResult = {
  text: (text: string): CallToolResult => ({ content: [textContent(text)] }),
  /** A tool-level failure the model should see, not a protocol error. */
  error: (message: string): CallToolResult => ({ content: [textContent(message)], isError: true }),
  /** `value` as TOON text, and as `structuredContent` when it is an object. */
  toon: (value: JsonValue): CallToolResult => ({
    content: [toonContent(value)],
    ...(isObject(value) ? { structuredContent: value } : {}),
  }),
};

type Outgoing = JsonObject;

const PARSE_ERROR = -32700;
const INVALID_REQUEST = -32600;
const METHOD_NOT_FOUND = -32601;
const INVALID_PARAMS = -32602;
const INTERNAL_ERROR = -32603;

/** One MCP session: create one per connection. */
export class McpServer {
  private initialized = false;

  constructor(private readonly service: McpService) {}

  /** Answer one line of newline-delimited JSON; undefined when there is none. */
  async handleLine(line: string): Promise<string | undefined> {
    let message: unknown;
    try {
      message = JSON.parse(line);
    } catch {
      return JSON.stringify(errorResponse(null, PARSE_ERROR, 'Parse error'));
    }
    const answer = await this.handleMessage(message);
    return answer === undefined ? undefined : JSON.stringify(answer);
  }

  /** Answer one decoded message; undefined for a notification or response. */
  async handleMessage(message: unknown): Promise<Outgoing | undefined> {
    if (!isObject(message) || message.jsonrpc !== '2.0') {
      return errorResponse(null, INVALID_REQUEST, 'Invalid Request');
    }
    // A response to a request this server never sends is ignored.
    if (!('method' in message) && ('result' in message || 'error' in message)) return undefined;

    const { method, params } = message;
    const hasId = 'id' in message;
    const id = message.id;
    if (
      typeof method !== 'string' ||
      (hasId && typeof id !== 'string' && typeof id !== 'number') ||
      (params !== undefined && !isObject(params))
    ) {
      return errorResponse(null, INVALID_REQUEST, 'Invalid Request');
    }
    // Notifications (`notifications/initialized`, `notifications/cancelled`)
    // need no answer, and none changes what this server does.
    if (!hasId) return undefined;

    try {
      const result = await this.dispatch(method, (params ?? {}) as JsonObject);
      return { jsonrpc: '2.0', id: id as RequestId, result };
    } catch (error) {
      if (error instanceof McpError) {
        return errorResponse(id as RequestId, error.code, error.message, error.data);
      }
      return errorResponse(id as RequestId, INTERNAL_ERROR, 'Internal error');
    }
  }

  private async dispatch(method: string, params: JsonObject): Promise<JsonObject> {
    if (method === 'ping') return {};
    if (method === 'initialize') return this.initialize();
    if (!this.initialized) throw new McpError(INVALID_REQUEST, 'Server not initialized');

    const { tools, resources, prompts } = this.service;
    switch (method) {
      case 'tools/list':
        if (tools) return { tools: (await tools.list()) as unknown as JsonValue[] };
        break;
      case 'tools/call':
        if (tools) {
          const name = requireString(params, 'name');
          const args = params.arguments ?? {};
          if (!isObject(args)) throw McpError.invalidParams('arguments must be an object');
          return (await tools.call(name, args)) as unknown as JsonObject;
        }
        break;
      case 'resources/list':
        if (resources) return { resources: (await resources.list()) as unknown as JsonValue[] };
        break;
      case 'resources/read':
        if (resources) {
          return (await resources.read(requireString(params, 'uri'))) as unknown as JsonObject;
        }
        break;
      case 'prompts/list':
        if (prompts) return { prompts: (await prompts.list()) as unknown as JsonValue[] };
        break;
      case 'prompts/get':
        if (prompts) {
          const name = requireString(params, 'name');
          const args = params.arguments ?? {};
          if (!isObject(args) || !Object.values(args).every((value) => typeof value === 'string')) {
            throw McpError.invalidParams('arguments must map names to strings');
          }
          return (await prompts.get(name, args as Record<string, string>)) as unknown as JsonObject;
        }
        break;
    }
    throw new McpError(METHOD_NOT_FOUND, 'Method not found');
  }

  private initialize(): JsonObject {
    this.initialized = true;
    const { tools, resources, prompts, serverInfo, instructions } = this.service;
    const capabilities: JsonObject = {};
    if (tools) capabilities.tools = { listChanged: false };
    if (resources) capabilities.resources = { subscribe: false, listChanged: false };
    if (prompts) capabilities.prompts = { listChanged: false };
    // This server speaks one revision; a client asking for another decides
    // from this answer whether it can continue.
    return {
      protocolVersion: MCP_PROTOCOL_VERSION,
      capabilities,
      serverInfo: serverInfo as unknown as JsonObject,
      ...(instructions === undefined ? {} : { instructions }),
    };
  }
}

function requireString(params: JsonObject, key: string): string {
  const value = params[key];
  if (typeof value !== 'string') throw McpError.invalidParams(`${key} must be a string`);
  return value;
}

function errorResponse(
  id: RequestId | null,
  code: number,
  message: string,
  data?: JsonValue
): Outgoing {
  return {
    jsonrpc: '2.0',
    id,
    error: { code, message, ...(data === undefined ? {} : { data }) },
  };
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
