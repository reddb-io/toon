import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createMcpDispatcher, normalizeMcpCoreValue } from '../dist/index.js';

test('MCP normalization omits undefined object properties and rejects unsupported arrays', () => {
  assert.deepEqual(
    normalizeMcpCoreValue({ title: undefined, nested: { value: true, optional: undefined } }),
    { nested: { value: true } }
  );
  assert.throws(() => normalizeMcpCoreValue([1, undefined]), /not representable/);
  assert.throws(() => normalizeMcpCoreValue(new Date()), /not representable/);
});

test('MCP dispatcher normalizes service output and maps invalid output to Internal Error', async () => {
  let invalid = false;
  const service = {
    serverInfo: () => ({ name: 'fixture', version: '1', title: undefined }),
    listTools: () =>
      invalid
        ? [undefined]
        : [{ name: 'tool', title: undefined, inputSchema: { type: 'object', optional: undefined } }],
    listResources: () => [],
    listPrompts: () => [],
    readResource: () => null,
    getPrompt: () => null,
    callTool: () => ({ resultType: 'complete', content: [] }),
  };
  const server = createMcpDispatcher(service);

  const valid = await server.dispatchEntry({ toonrpc: '1.0', method: 'tools/list', id: 1 });
  assert.deepEqual(valid, {
    toonrpc: '1.0',
    result: {
      resultType: 'complete',
      items: [{ name: 'tool', inputSchema: { type: 'object' } }],
    },
    id: 1,
  });

  invalid = true;
  const rejected = await server.dispatchEntry({ toonrpc: '1.0', method: 'tools/list', id: 2 });
  assert.deepEqual(rejected, {
    toonrpc: '1.0',
    error: { code: -32603, message: 'Internal error' },
    id: 2,
  });
});
