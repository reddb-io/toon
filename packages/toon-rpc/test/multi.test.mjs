import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Server } from '../dist/index.js';
import { MultiRpc, detectProtocol, contentTypeFor } from '../dist/multi.js';
import { decode } from '@reddb-io/toon';

const dec = (bytes) => new TextDecoder().decode(bytes);

function calculator() {
  const server = new Server();
  server.register('add', async (params) => {
    const [a, b] = Array.isArray(params) ? params : [params.a, params.b];
    return a + b;
  });
  let notified = 0;
  server.register('notify_hello', async () => { notified += 1; return null; });
  return { server, notifiedCount: () => notified };
}

test('detectProtocol: content-type wins over sniffing', () => {
  assert.equal(detectProtocol('toonrpc: "1.0"', 'application/json'), 'jsonrpc');
  assert.equal(detectProtocol('{"jsonrpc":"2.0"}', 'application/toon; charset=utf-8'), 'toonrpc');
});

test('detectProtocol: sniffs jsonrpc marker in the head, defaults to toonrpc', () => {
  assert.equal(detectProtocol('{"jsonrpc":"2.0","method":"add","id":1}'), 'jsonrpc');
  assert.equal(detectProtocol('  [{"jsonrpc":"2.0","method":"add","id":1}]'), 'jsonrpc');
  assert.equal(detectProtocol('toonrpc: "1.0"\nmethod: add\nid: 1'), 'toonrpc');
  assert.equal(detectProtocol('{"data": 1}'), 'toonrpc');
  assert.equal(detectProtocol(''), 'toonrpc');
});

test('contentTypeFor names the negotiation MIME types', () => {
  assert.equal(contentTypeFor('jsonrpc'), 'application/json');
  assert.equal(contentTypeFor('toonrpc'), 'application/toon');
});

test('a JSON-RPC request is answered in JSON-RPC', async () => {
  const { server } = calculator();
  const multi = new MultiRpc(server);
  const { protocol, body } = await multi.handleWithProtocol(
    '{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}'
  );
  assert.equal(protocol, 'jsonrpc');
  assert.deepEqual(JSON.parse(dec(body)), { jsonrpc: '2.0', result: 5, id: 1 });
});

test('a TOON-RPC request is answered in TOON-RPC', async () => {
  const { server } = calculator();
  const multi = new MultiRpc(server);
  const { protocol, body } = await multi.handleWithProtocol(
    'toonrpc: "1.0"\nmethod: add\nparams[2]: 2,3\nid: 1'
  );
  assert.equal(protocol, 'toonrpc');
  const parsed = decode(dec(body));
  assert.deepEqual(parsed, { toonrpc: '1.0', result: 5, id: 1 });
});

test('one registry serves both dialects', async () => {
  const { server } = calculator();
  const multi = new MultiRpc(server);
  const json = JSON.parse(dec(await multi.handle('{"jsonrpc":"2.0","method":"add","params":{"a":20,"b":22},"id":"x"}')));
  const toon = decode(dec(await multi.handle('toonrpc: "1.0"\nmethod: add\nparams:\n  a: 20\n  b: 22\nid: x')));
  assert.equal(json.result, 42);
  assert.equal(toon.result, 42);
});

test('a JSON-RPC notification runs its handler and returns nothing', async () => {
  const { server, notifiedCount } = calculator();
  const multi = new MultiRpc(server);
  const body = await multi.handle('{"jsonrpc":"2.0","method":"notify_hello","params":[]}');
  assert.equal(body.length, 0);
  assert.equal(notifiedCount(), 1);
});

test('a JSON-RPC batch echoes the batch shape and drops notification slots', async () => {
  const { server } = calculator();
  const multi = new MultiRpc(server);
  const body = await multi.handle(JSON.stringify([
    { jsonrpc: '2.0', method: 'add', params: [1, 2], id: 1 },
    { jsonrpc: '2.0', method: 'notify_hello', params: [] },
    { jsonrpc: '2.0', method: 'missing', params: [], id: 2 },
  ]));
  const parsed = JSON.parse(dec(body));
  assert.equal(parsed.length, 2);
  assert.deepEqual(parsed[0], { jsonrpc: '2.0', result: 3, id: 1 });
  assert.equal(parsed[1].error.code, -32601);
});

test('a JSON parse error answers -32700 in JSON', async () => {
  const { server } = calculator();
  const multi = new MultiRpc(server);
  const parsed = JSON.parse(dec(await multi.handle('{"jsonrpc": nope', 'application/json')));
  assert.equal(parsed.error.code, -32700);
  assert.equal(parsed.id, null);
});

test('a wrong jsonrpc version is refused as -32600', async () => {
  const { server } = calculator();
  const multi = new MultiRpc(server);
  const parsed = JSON.parse(dec(await multi.handle('{"jsonrpc":"1.0","method":"add","id":7}')));
  assert.equal(parsed.error.code, -32600);
  assert.equal(parsed.id, 7);
});

test('Server: id null is an id, id absent is a notification', async () => {
  const { server } = calculator();
  const withNull = await server.dispatchEntry({ toonrpc: '1.0', method: 'add', params: [1, 1], id: null });
  assert.deepEqual(withNull, { toonrpc: '1.0', result: 2, id: null });
  const absent = await server.dispatchEntry({ toonrpc: '1.0', method: 'add', params: [1, 1] });
  assert.equal(absent, undefined);
});

test('Server: unknown-method and throwing notifications are dropped silently', async () => {
  const server = new Server();
  server.register('boom', async () => { throw new Error('kaput'); });
  assert.equal(await server.dispatchEntry({ toonrpc: '1.0', method: 'missing' }), undefined);
  assert.equal(await server.dispatchEntry({ toonrpc: '1.0', method: 'boom' }), undefined);
  const answered = await server.dispatchEntry({ toonrpc: '1.0', method: 'boom', id: 3 });
  assert.equal(answered.error.code, -32603);
  assert.equal(answered.error.message, 'kaput');
});
