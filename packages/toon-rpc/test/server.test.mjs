import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Server } from '../dist/index.js';

function calculator() {
  const server = new Server();
  server.register('add', async (params) => {
    const [a, b] = Array.isArray(params) ? params : [params.a, params.b];
    return a + b;
  });
  return server;
}

test('id null is an id while an absent id is a notification', async () => {
  const server = calculator();
  const withNull = await server.dispatchEntry({
    toonrpc: '1.0',
    method: 'add',
    params: [1, 1],
    id: null,
  });
  assert.deepEqual(withNull, { toonrpc: '1.0', result: 2, id: null });

  const absent = await server.dispatchEntry({
    toonrpc: '1.0',
    method: 'add',
    params: [1, 1],
  });
  assert.equal(absent, undefined);
});

test('unknown-method and throwing notifications are dropped silently', async () => {
  const server = new Server();
  server.register('boom', async () => {
    throw new Error('kaput');
  });

  assert.equal(await server.dispatchEntry({ toonrpc: '1.0', method: 'missing' }), undefined);
  assert.equal(await server.dispatchEntry({ toonrpc: '1.0', method: 'boom' }), undefined);

  const answered = await server.dispatchEntry({ toonrpc: '1.0', method: 'boom', id: 3 });
  assert.equal(answered.error.code, -32603);
  assert.equal(answered.error.message, 'kaput');
});
