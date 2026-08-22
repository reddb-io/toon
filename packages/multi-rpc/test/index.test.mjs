import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MultiRpc, Server, detectProtocol } from '../dist/index.js';

test('standalone package exports a working multi-protocol server', async () => {
  const server = new Server();
  server.register('echo', async (params) => params);

  const response = await new MultiRpc(server).handle(
    '{"jsonrpc":"2.0","method":"echo","params":{"name":"Ada"},"id":1}'
  );

  assert.equal(detectProtocol(response, 'application/json'), 'jsonrpc');
  assert.deepEqual(JSON.parse(new TextDecoder().decode(response)), {
    jsonrpc: '2.0',
    result: { name: 'Ada' },
    id: 1,
  });
});
