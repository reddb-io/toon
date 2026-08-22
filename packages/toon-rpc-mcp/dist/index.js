import { decode, encode } from '@reddb-io/toon';
import { Server } from '@reddb-io/toon-rpc';
export const MCP_PROTOCOL_VERSION = '2026-07-28';
export const MCP_NS = 'io.modelcontextprotocol';
export const CallToolResponse = {
    text: (text) => ({
        resultType: 'complete',
        content: [{ type: 'text', text }],
    }),
    error: (message) => ({
        resultType: 'complete',
        content: [{ type: 'text', text: message }],
        isError: true,
    }),
};
export function createMcpDispatcher(service) {
    const server = new Server();
    server.register('server/discover', async () => {
        const info = service.serverInfo();
        const response = {
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
        return response;
    });
    server.register('tools/list', async () => {
        return {
            resultType: 'complete',
            items: service.listTools(),
        };
    });
    server.register('tools/call', async (params) => {
        const p = params;
        if (!p || typeof p.name !== 'string') {
            return CallToolResponse.error('missing tool name');
        }
        return (await service.callTool(p.name, p.arguments ?? {}));
    });
    server.register('resources/list', async () => {
        return {
            resultType: 'complete',
            items: service.listResources(),
        };
    });
    server.register('resources/read', async (params) => {
        const p = params;
        if (!p || typeof p.uri !== 'string') {
            throw new Error('missing resource uri');
        }
        return await service.readResource(p.uri);
    });
    server.register('prompts/list', async () => {
        return {
            resultType: 'complete',
            items: service.listPrompts(),
        };
    });
    server.register('prompts/get', async (params) => {
        const p = params;
        if (!p || typeof p.name !== 'string') {
            throw new Error('missing prompt name');
        }
        return await service.getPrompt(p.name, p.arguments);
    });
    return server;
}
export function encodeMcpMessage(value) {
    return encode(value);
}
export function decodeMcpMessage(text) {
    return decode(text);
}
//# sourceMappingURL=index.js.map