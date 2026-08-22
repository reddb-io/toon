import { decode, encode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';
import { Server } from '@reddb-io/toon-rpc';

export const MCP_PROTOCOL_VERSION = '2026-07-28';
export const MCP_NS = 'io.modelcontextprotocol';

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

export interface Capability {}

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
  arguments?: Array<{ name: string; description?: string; required?: boolean; [k: string]: unknown }>;
  [k: string]: unknown;
}

export interface CallToolResponse {
  resultType: 'complete';
  content: Array<{ type: string; [k: string]: JsonValue }>;
  isError?: boolean;
}

export const CallToolResponse = {
  text: (text: string): CallToolResponse => ({
    resultType: 'complete',
    content: [{ type: 'text', text }],
  }),
  error: (message: string): CallToolResponse => ({
    resultType: 'complete',
    content: [{ type: 'text', text: message }],
    isError: true,
  }),
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

export function createMcpDispatcher(service: McpService): Server {
  const server = new Server();

  server.register('server/discover', async () => {
    const info = service.serverInfo();
    const response: DiscoverResponse = {
      resultType: 'complete',
      supportedVersions: [MCP_PROTOCOL_VERSION],
      capabilities: {
        tools: { listChanged: false },
        resources: {},
        prompts: {},
      },
      _meta: {
        [MCP_NS + '/serverInfo']: info,
      },
      ttlMs: 3600000,
      cacheScope: 'public',
    };
    return response as unknown as JsonValue;
  });

  server.register('tools/list', async () => {
    return {
      resultType: 'complete',
      items: service.listTools(),
    } as unknown as JsonValue;
  });

  server.register('tools/call', async (params: unknown) => {
    const p = params as { name: string; arguments?: JsonValue };
    if (!p || typeof p.name !== 'string') {
      return CallToolResponse.error('missing tool name') as unknown as JsonValue;
    }
    return (await service.callTool(p.name, p.arguments ?? {})) as unknown as JsonValue;
  });

  server.register('resources/list', async () => {
    return {
      resultType: 'complete',
      items: service.listResources(),
    } as unknown as JsonValue;
  });

  server.register('resources/read', async (params: unknown) => {
    const p = params as { uri: string };
    if (!p || typeof p.uri !== 'string') {
      throw new Error('missing resource uri');
    }
    return await service.readResource(p.uri);
  });

  server.register('prompts/list', async () => {
    return {
      resultType: 'complete',
      items: service.listPrompts(),
    } as unknown as JsonValue;
  });

  server.register('prompts/get', async (params: unknown) => {
    const p = params as { name: string; arguments?: JsonValue };
    if (!p || typeof p.name !== 'string') {
      throw new Error('missing prompt name');
    }
    return await service.getPrompt(p.name, p.arguments);
  });

  return server;
}

export function encodeMcpMessage(value: JsonValue): string {
  return encode(value);
}

export function decodeMcpMessage(text: string): JsonValue {
  return decode(text) as JsonValue;
}
