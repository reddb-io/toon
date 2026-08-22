import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  MultiRpc,
  Server,
  contentTypeFor,
  decodeMessage,
  detectProtocol,
  encodeMessage,
} from '../dist/index.js';
import { decode } from '@reddb-io/toon';

const dec = (bytes) => new TextDecoder().decode(bytes);

function calculator() {
  const server = new Server();
  server.register('add', async (params) => {
    const [a, b] = Array.isArray(params) ? params : [params.a, params.b];
    return a + b;
  });
  let notified = 0;
  server.register('notify_hello', async () => {
    notified += 1;
    return null;
  });
  return { server, notifiedCount: () => notified };
}

test('toon-rpc does not expose the MultiRpc subpath', async () => {
  await assert.rejects(import('@reddb-io/toon-rpc/multi'), {
    code: 'ERR_PACKAGE_PATH_NOT_EXPORTED',
  });
});

test('content-type wins over protocol sniffing', () => {
  assert.equal(detectProtocol('toonrpc: "1.0"', 'application/json'), 'jsonrpc');
  assert.equal(detectProtocol('{"jsonrpc":"2.0"}', 'application/toon; charset=utf-8'), 'toonrpc');
});

test('detectProtocol sniffs JSON-RPC and defaults to TOON-RPC', () => {
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
  const { protocol, body } = await new MultiRpc(server).handleWithProtocol(
    '{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}'
  );
  assert.equal(protocol, 'jsonrpc');
  assert.deepEqual(JSON.parse(dec(body)), { jsonrpc: '2.0', result: 5, id: 1 });
});

test('a TOON-RPC request is answered in TOON-RPC', async () => {
  const { server } = calculator();
  const { protocol, body } = await new MultiRpc(server).handleWithProtocol(
    'toonrpc: "1.0"\nmethod: add\nparams[2]: 2,3\nid: 1'
  );
  assert.equal(protocol, 'toonrpc');
  assert.deepEqual(decode(dec(body)), { toonrpc: '1.0', result: 5, id: 1 });
});

test('one registry serves both dialects', async () => {
  const { server } = calculator();
  const multi = new MultiRpc(server);
  const json = JSON.parse(
    dec(await multi.handle('{"jsonrpc":"2.0","method":"add","params":{"a":20,"b":22},"id":"x"}'))
  );
  const toon = decode(
    dec(await multi.handle('toonrpc: "1.0"\nmethod: add\nparams:\n  a: 20\n  b: 22\nid: x'))
  );
  assert.equal(json.result, 42);
  assert.equal(toon.result, 42);
});

test('a JSON-RPC notification runs its handler and returns nothing', async () => {
  const { server, notifiedCount } = calculator();
  const body = await new MultiRpc(server).handle(
    '{"jsonrpc":"2.0","method":"notify_hello","params":[]}'
  );
  assert.equal(body.length, 0);
  assert.equal(notifiedCount(), 1);
});

test('a JSON-RPC batch preserves shape and drops notification slots', async () => {
  const { server } = calculator();
  const body = await new MultiRpc(server).handle(
    JSON.stringify([
      { jsonrpc: '2.0', method: 'add', params: [1, 2], id: 1 },
      { jsonrpc: '2.0', method: 'notify_hello', params: [] },
      { jsonrpc: '2.0', method: 'missing', params: [], id: 2 },
    ])
  );
  const parsed = JSON.parse(dec(body));
  assert.equal(parsed.length, 2);
  assert.deepEqual(parsed[0], { jsonrpc: '2.0', result: 3, id: 1 });
  assert.equal(parsed[1].error.code, -32601);
});

test('JSON parse and version errors use JSON-RPC error codes', async () => {
  const { server } = calculator();
  const multi = new MultiRpc(server);
  const parseError = JSON.parse(dec(await multi.handle('{"jsonrpc": nope', 'application/json')));
  assert.equal(parseError.error.code, -32700);
  assert.equal(parseError.id, null);

  const versionError = JSON.parse(
    dec(await multi.handle('{"jsonrpc":"1.0","method":"add","id":7}'))
  );
  assert.equal(versionError.error.code, -32600);
  assert.equal(versionError.id, 7);
});

test('message helpers translate envelopes in both directions', () => {
  assert.equal(
    encodeMessage({ toonrpc: '1.0', method: 'ping', id: 1 }, 'jsonrpc'),
    '{"jsonrpc":"2.0","method":"ping","id":1}'
  );
  assert.deepEqual(decodeMessage('{"jsonrpc":"2.0","result":true,"id":1}'), {
    protocol: 'jsonrpc',
    message: { jsonrpc: '2.0', result: true, id: 1 },
  });
});
