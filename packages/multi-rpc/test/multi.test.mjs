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
import { decode, encode } from '@reddb-io/toon';

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
  assert.equal(detectProtocol('toonrpc: "1.0"', 'text/application/json'), 'toonrpc');
  assert.equal(detectProtocol('toonrpc: "1.0"', 'application/json-patch+json'), 'toonrpc');
});

test('detectProtocol sniffs JSON-RPC and defaults to TOON-RPC', () => {
  assert.equal(detectProtocol('{"jsonrpc":"2.0","method":"add","id":1}'), 'jsonrpc');
  assert.equal(detectProtocol('  [{"jsonrpc":"2.0","method":"add","id":1}]'), 'jsonrpc');
  assert.equal(detectProtocol('toonrpc: "1.0"\nmethod: add\nid: 1'), 'toonrpc');
  assert.equal(detectProtocol('{"data": 1}'), 'toonrpc');
  assert.equal(
    detectProtocol(JSON.stringify({ padding: 'x'.repeat(200), jsonrpc: '2.0', method: 'add' })),
    'jsonrpc'
  );
  assert.equal(detectProtocol('{"nested":{"jsonrpc":"2.0"}}'), 'toonrpc');
  assert.equal(detectProtocol(''), 'toonrpc');
});

test('JSON byte sniffing reuses the fatal decode and parsed document', async () => {
  const { server } = calculator();
  const raw = new TextEncoder().encode(
    '{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}'
  );
  const parse = JSON.parse;
  let parses = 0;
  let body;
  try {
    JSON.parse = (...args) => {
      parses += 1;
      return parse(...args);
    };
    body = await new MultiRpc(server).handle(raw);
  } finally {
    JSON.parse = parse;
  }
  assert.equal(parses, 1);
  assert.deepEqual(JSON.parse(dec(body)), { jsonrpc: '2.0', result: 5, id: 1 });
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
  assert.equal(versionError.id, null);
});

test('JSON translation preserves absent params and id versus explicit null', async () => {
  const server = new Server();
  const seen = [];
  server.register('inspect', async (params, id) => {
    seen.push({ params, id });
    return null;
  });
  const multi = new MultiRpc(server);

  const notification = await multi.handle('{"jsonrpc":"2.0","method":"inspect"}');
  const request = JSON.parse(
    dec(await multi.handle('{"jsonrpc":"2.0","method":"inspect","id":null}'))
  );

  assert.equal(notification.length, 0);
  assert.deepEqual(request, { jsonrpc: '2.0', result: null, id: null });
  assert.deepEqual(seen, [
    { params: undefined, id: undefined },
    { params: undefined, id: null },
  ]);
});

test('JSON envelopes reuse core recursive validation and isolate malformed batch entries', async () => {
  const { server } = calculator();
  const body = await new MultiRpc(server).handle(
    JSON.stringify([
      {
        jsonrpc: '2.0',
        method: 'add',
        params: [1, 1],
        id: 1,
        toonrpc: Number.MAX_SAFE_INTEGER + 1,
      },
      { jsonrpc: '2.0', method: 'add', params: [2, 3], id: 2 },
    ]),
    'application/json'
  );
  const response = JSON.parse(dec(body));
  assert.equal(response[0].error.code, -32600);
  assert.equal(response[0].id, null);
  assert.deepEqual(response[1], { jsonrpc: '2.0', result: 5, id: 2 });
});

test('JSON response translation preserves result null and Error Object data null', async () => {
  const server = new Server();
  server.register('null', async () => null);
  server.register('fail', async () => {
    const { RpcError } = await import('@reddb-io/toon-rpc');
    throw new RpcError(1001, 'failure', null);
  });
  const multi = new MultiRpc(server);

  const result = JSON.parse(dec(await multi.handle('{"jsonrpc":"2.0","method":"null","id":1}')));
  const error = JSON.parse(dec(await multi.handle('{"jsonrpc":"2.0","method":"fail","id":2}')));
  assert.equal(Object.hasOwn(result, 'result'), true);
  assert.equal(result.result, null);
  assert.equal(Object.hasOwn(error.error, 'data'), true);
  assert.equal(error.error.data, null);
});

test('deep batch response stays valid in JSON but is isolated by TOON preflight', async () => {
  const server = new Server();
  let deep = null;
  for (let depth = 0; depth < 999; depth += 1) deep = { next: deep };
  server.register('deep', async () => deep);
  server.register('ok', async () => 'sibling');
  const multi = new MultiRpc(server);
  const requests = [
    { method: 'deep', id: 1 },
    { method: 'ok', id: 2 },
  ];

  const json = JSON.parse(
    dec(
      await multi.handle(
        JSON.stringify(requests.map((entry) => ({ jsonrpc: '2.0', ...entry }))),
        'application/json'
      )
    )
  );
  assert.equal(Object.hasOwn(json[0], 'result'), true);
  let cursor = json[0].result;
  for (let depth = 0; depth < 999; depth += 1) cursor = cursor.next;
  assert.equal(cursor, null);
  assert.deepEqual(json[1], { jsonrpc: '2.0', result: 'sibling', id: 2 });

  const toon = decode(
    dec(
      await multi.handle(
        encode(requests.map((entry) => ({ toonrpc: '1.0', ...entry }))),
        'application/toon'
      )
    )
  );
  assert.equal(Array.isArray(toon), true);
  assert.deepEqual(toon[0], {
    toonrpc: '1.0',
    error: { code: -32603, message: 'Internal error' },
    id: 1,
  });
  assert.deepEqual(toon[1], { toonrpc: '1.0', result: 'sibling', id: 2 });
});

test('JSON preflight isolates an unstringifiable deep response and preserves siblings', async () => {
  const server = new Server();
  let tooDeep = null;
  for (let depth = 0; depth < 100; depth += 1) tooDeep = { next: tooDeep };
  server.register('too-deep', async () => tooDeep);
  server.register('ok', async () => true);
  const request = JSON.stringify([
    { jsonrpc: '2.0', method: 'too-deep', id: 1 },
    { jsonrpc: '2.0', method: 'ok', id: 2 },
  ]);
  const stringify = JSON.stringify;
  let response;
  try {
    JSON.stringify = (value, ...args) => {
      if (value?.jsonrpc === '2.0' && value.id === 1 && Object.hasOwn(value, 'result')) {
        throw new RangeError('fixture JSON encoder depth failure');
      }
      return stringify(value, ...args);
    };
    response = JSON.parse(
      dec(await new MultiRpc(server).handle(request, 'application/json'))
    );
  } finally {
    JSON.stringify = stringify;
  }
  assert.deepEqual(response[0], {
    jsonrpc: '2.0',
    error: { code: -32603, message: 'Internal error' },
    id: 1,
  });
  assert.deepEqual(response[1], { jsonrpc: '2.0', result: true, id: 2 });
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

test('message helpers translate JSON and TOON batches element by element', () => {
  const messages = [
    { jsonrpc: '2.0', method: 'ping', id: 1 },
    { jsonrpc: '2.0', result: null, id: 1 },
  ];
  const json = encodeMessage(messages, 'jsonrpc');
  const toon = encodeMessage(messages, 'toonrpc');

  assert.deepEqual(decodeMessage(json), { protocol: 'jsonrpc', message: messages });
  assert.deepEqual(decodeMessage(toon), { protocol: 'toonrpc', message: messages });
  assert.throws(() => decodeMessage('[2]:\n  - 1\n  - 2'), /entries must be objects/);
});
