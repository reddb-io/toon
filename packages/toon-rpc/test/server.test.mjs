import assert from 'node:assert/strict';
import { test } from 'node:test';
import { decode, encode } from '@reddb-io/toon';
import { RpcError, Server } from '../dist/index.js';

const text = (bytes) => new TextDecoder().decode(bytes);
const decoded = (bytes) => decode(text(bytes));

async function handleValue(server, value) {
  return decoded(await server.handleText(encode(value)));
}

function fixtureServer() {
  const server = new Server();
  const calls = new Map();
  const register = (method, handler) => {
    server.register(method, async (params, id) => {
      calls.set(method, (calls.get(method) ?? 0) + 1);
      return handler(params, id);
    });
  };

  register('echo', async (params) => params);
  register('ok', async () => ({ ok: true }));
  register('null', async () => null);
  register('observe', async () => null);
  register('fail', async () => {
    throw new RpcError(1000, 'fixture failure');
  });
  register('fail-data', async () => {
    throw new RpcError(1001, 'fixture failure with data', null);
  });
  register('boom', async () => {
    throw new Error('private detail');
  });

  return { server, calls };
}

test('invalid UTF-8 and invalid TOON are Parse Error, not Invalid Request', async () => {
  const server = new Server();
  const utf8 = decoded(await server.handle(Uint8Array.from([0xff, 0xfe])));
  const syntax = decoded(await server.handleText('toonrpc: "unterminated'));

  for (const response of [utf8, syntax]) {
    assert.equal(response.error.code, -32700);
    assert.equal(response.id, null);
    assert.deepEqual(Object.keys(response).sort(), ['error', 'id', 'toonrpc']);
    assert.equal(Object.hasOwn(response.error, 'data'), false);
  }
});

test('handleText rejects lone surrogates in methods and ids as Parse Error', async () => {
  const { server, calls } = fixtureServer();
  const method = decoded(
    await server.handleText('toonrpc: "1.0"\nmethod: "\ud800"\nid: 1')
  );
  const id = decoded(
    await server.handleText('toonrpc: "1.0"\nmethod: "ok"\nid: "\udfff"')
  );

  assert.equal(method.error.code, -32700);
  assert.equal(id.error.code, -32700);
  assert.equal(method.id, null);
  assert.equal(id.id, null);
  assert.equal(calls.size, 0);
});

test('decoded malformed envelopes produce uncorrelated Invalid Request errors', async () => {
  const { server, calls } = fixtureServer();
  const malformed = [
    1,
    null,
    {},
    { method: 'ok', id: 1 },
    { toonrpc: '0.9', method: 'ok', id: 2 },
    { toonrpc: 1, method: 'ok', id: 3 },
    { toonrpc: '1.0', id: 4 },
    { toonrpc: '1.0', method: '', id: 5 },
    { toonrpc: '1.0', method: 1, id: 6 },
    { toonrpc: '1.0', method: 'echo', params: null, id: 7 },
    { toonrpc: '1.0', method: 'echo', params: 1, id: 8 },
    { toonrpc: '1.0', method: 'ok', id: true },
    { toonrpc: '1.0', method: 'ok', id: {} },
    { toonrpc: '1.0', method: 'ok', id: [] },
    { toonrpc: '1.0', method: 'ok', id: 1.5 },
    { toonrpc: '1.0', method: 'ok', id: Number.MAX_SAFE_INTEGER + 1 },
  ];

  for (const entry of malformed) {
    const response = await server.dispatchEntry(entry);
    assert.equal(response.error.code, -32600);
    assert.equal(response.id, null);
  }
  assert.equal(calls.size, 0);
});

test('unsafe integer and non-finite numeric wire tokens are invalid envelopes', async () => {
  const { server, calls } = fixtureServer();
  const unsafe = decoded(
    await server.handleText('toonrpc: "1.0"\nmethod: "ok"\nid: 9007199254740993')
  );
  const nonFinite = decoded(
    await server.handleText('toonrpc: "1.0"\nmethod: "echo"\nparams[1]: 1e400\nid: 32')
  );

  assert.equal(unsafe.error.code, -32600);
  assert.equal(unsafe.id, null);
  assert.equal(nonFinite.error.code, -32600);
  assert.equal(nonFinite.id, null);
  assert.equal(calls.size, 0);
});

test('absence of id and params is preserved while explicit null id is a request', async () => {
  const server = new Server();
  const seen = [];
  server.register('inspect', async (params, id) => {
    seen.push({ params, id });
    return { paramsAbsent: params === undefined };
  });

  const request = await server.dispatchEntry({ toonrpc: '1.0', method: 'inspect', id: null });
  const notification = await server.dispatchEntry({ toonrpc: '1.0', method: 'inspect' });

  assert.deepEqual(request, {
    toonrpc: '1.0',
    result: { paramsAbsent: true },
    id: null,
  });
  assert.equal(notification, undefined);
  assert.deepEqual(seen, [
    { params: undefined, id: null },
    { params: undefined, id: undefined },
  ]);
});

test('dispatch uses the request snapshot rather than rereading a hostile envelope', async () => {
  const server = new Server();
  server.register('captured', async (params, id) => ({ params, id }));
  const source = { toonrpc: '1.0', method: 'captured', params: { value: 1 }, id: 'x' };
  const request = new Proxy(source, {
    getOwnPropertyDescriptor(target, key) {
      return Reflect.getOwnPropertyDescriptor(target, key);
    },
    get() {
      throw new Error('validated request properties must not be reread');
    },
  });

  assert.deepEqual(await server.dispatchEntry(request), {
    toonrpc: '1.0',
    result: { params: { value: 1 }, id: 'x' },
    id: 'x',
  });
});

test('malformed objects without id are not notifications and do not invoke handlers', async () => {
  const { server, calls } = fixtureServer();
  const response = await server.dispatchEntry({ toonrpc: '1.0', params: { source: 'bad' } });
  assert.equal(response.error.code, -32600);
  assert.equal(response.id, null);
  assert.equal(calls.size, 0);
});

test('valid unknown members are ignored but invalid recursively reachable values reject the request', async () => {
  const { server, calls } = fixtureServer();
  const valid = await server.dispatchEntry({
    toonrpc: '1.0',
    method: 'ok',
    id: 9,
    trace: { sample: 0.5 },
  });
  const invalid = await server.dispatchEntry({
    toonrpc: '1.0',
    method: 'ok',
    id: 10,
    trace: { unsafe: Number.MAX_SAFE_INTEGER + 1 },
  });

  assert.deepEqual(valid, { toonrpc: '1.0', result: { ok: true }, id: 9 });
  assert.equal(invalid.error.code, -32600);
  assert.equal(invalid.id, null);
  assert.equal(calls.get('ok'), 1);
});

test('method, handler and RpcError responses preserve correlation and exact branches', async () => {
  const { server } = fixtureServer();
  const missing = await server.dispatchEntry({ toonrpc: '1.0', method: 'missing', id: 'm' });
  const failure = await server.dispatchEntry({ toonrpc: '1.0', method: 'fail', id: 6 });
  const withData = await server.dispatchEntry({ toonrpc: '1.0', method: 'fail-data', id: 7 });
  const internal = await server.dispatchEntry({ toonrpc: '1.0', method: 'boom', id: 17 });
  const nullResult = await server.dispatchEntry({ toonrpc: '1.0', method: 'null', id: 5 });

  assert.deepEqual(missing, {
    toonrpc: '1.0',
    error: { code: -32601, message: 'Method not found' },
    id: 'm',
  });
  assert.deepEqual(failure.error, { code: 1000, message: 'fixture failure' });
  assert.deepEqual(withData.error, {
    code: 1001,
    message: 'fixture failure with data',
    data: null,
  });
  assert.deepEqual(internal.error, { code: -32603, message: 'Internal error' });
  assert.equal(Object.hasOwn(nullResult, 'result'), true);
  assert.equal(nullResult.result, null);
  assert.equal(Object.hasOwn(nullResult, 'error'), false);
});

test('RpcError enforces handler-legal reserved codes and accepts application and server codes', async () => {
  const server = new Server();
  const allowed = [-2147483648, -32769, -32603, -32602, -32099, -32000, -31999, 0, 2147483647];
  const rejected = [-2147483649, -32700, -32600, -32601, -32768, -32604, -32100, 2147483648];
  for (const code of [...allowed, ...rejected]) {
    server.register(String(code), async () => {
      throw new RpcError(code, `handler ${code}`, { private: true });
    });
  }
  server.register('undefined-data', async () => {
    throw new RpcError(1, 'bad', undefined);
  });

  for (const code of allowed) {
    const response = await server.dispatchEntry({ toonrpc: '1.0', method: String(code), id: code });
    assert.deepEqual(response.error, {
      code,
      message: `handler ${code}`,
      data: { private: true },
    });
  }
  for (const code of rejected) {
    const response = await server.dispatchEntry({ toonrpc: '1.0', method: String(code), id: code });
    assert.deepEqual(response.error, { code: -32603, message: 'Internal error' });
    assert.equal(response.id, code);
    assert.equal(Object.hasOwn(response.error, 'data'), false);
  }
  assert.deepEqual(
    (await server.dispatchEntry({ toonrpc: '1.0', method: 'undefined-data', id: 4 })).error,
    { code: -32603, message: 'Internal error' }
  );
});

test('valid notifications run exactly once and never emit errors', async () => {
  const { server, calls } = fixtureServer();
  assert.equal(await server.dispatchEntry({ toonrpc: '1.0', method: 'observe' }), undefined);
  assert.equal(await server.dispatchEntry({ toonrpc: '1.0', method: 'missing' }), undefined);
  assert.equal(await server.dispatchEntry({ toonrpc: '1.0', method: 'boom' }), undefined);
  assert.equal(calls.get('observe'), 1);
  assert.equal(calls.get('boom'), 1);
});

test('batches validate entries independently, omit notifications and preserve array shape', async () => {
  const { server, calls } = fixtureServer();
  const response = await handleValue(server, [
    { toonrpc: '0.9', method: 'ok', id: 30 },
    { toonrpc: '1.0', method: 'observe' },
    { toonrpc: '1.0', method: 'ok', id: 31 },
  ]);

  assert.equal(Array.isArray(response), true);
  assert.equal(response.length, 2);
  assert.equal(response[0].error.code, -32600);
  assert.equal(response[0].id, null);
  assert.deepEqual(response[1], { toonrpc: '1.0', result: { ok: true }, id: 31 });
  assert.equal(calls.get('observe'), 1);
  assert.equal(calls.get('ok'), 1);
});

test('one batch response stays an array, all notifications emit nothing, and empty batch is invalid', async () => {
  const { server } = fixtureServer();
  const one = await handleValue(server, [
    { toonrpc: '1.0', method: 'observe' },
    { toonrpc: '1.0', method: 'ok', id: 12 },
  ]);
  const none = await server.handleText(
    encode([
      { toonrpc: '1.0', method: 'observe' },
      { toonrpc: '1.0', method: 'observe', params: { source: 'batch' } },
    ])
  );
  const empty = await handleValue(server, []);

  assert.equal(Array.isArray(one), true);
  assert.equal(one.length, 1);
  assert.equal(none.length, 0);
  assert.equal(Array.isArray(empty), false);
  assert.equal(empty.error.code, -32600);
  assert.equal(empty.id, null);
});

test('invalid and non-encodable handler outputs become isolated correlated Internal Errors', async () => {
  const server = new Server();
  server.register('undefined', async () => undefined);
  server.register('unsafe', async () => ({ nested: Number.MAX_SAFE_INTEGER + 1 }));
  server.register('non-encodable', async () =>
    new Proxy(
      { ok: true },
      {
        getOwnPropertyDescriptor() {
          throw new Error('cannot inspect');
        },
      }
    )
  );
  server.register('ok', async () => 'survived');

  for (const method of ['undefined', 'unsafe']) {
    const response = await server.dispatchEntry({ toonrpc: '1.0', method, id: method });
    assert.deepEqual(response, {
      toonrpc: '1.0',
      error: { code: -32603, message: 'Internal error' },
      id: method,
    });
  }

  const batch = await handleValue(server, [
    { toonrpc: '1.0', method: 'non-encodable', id: 1 },
    { toonrpc: '1.0', method: 'ok', id: 2 },
  ]);
  assert.equal(batch[0].error.code, -32603);
  assert.equal(batch[0].id, 1);
  assert.deepEqual(batch[1], { toonrpc: '1.0', result: 'survived', id: 2 });
});

test('handler results are snapshotted before a sibling can mutate shared state', async () => {
  const server = new Server();
  const shared = { value: 'before' };
  server.register('read', async () => shared);
  server.register('mutate', async () => {
    shared.value = 'after';
    return true;
  });

  const batch = await handleValue(server, [
    { toonrpc: '1.0', method: 'read', id: 1 },
    { toonrpc: '1.0', method: 'mutate', id: 2 },
  ]);
  assert.deepEqual(batch[0], { toonrpc: '1.0', result: { value: 'before' }, id: 1 });
  assert.deepEqual(batch[1], { toonrpc: '1.0', result: true, id: 2 });
  assert.equal(shared.value, 'after');
});

test('stateful Proxies are read through descriptors once and responses use the snapshot', async () => {
  const server = new Server();
  let descriptorReads = 0;
  server.register('proxy', async () =>
    new Proxy(
      { value: 'target' },
      {
        getOwnPropertyDescriptor(target, key) {
          const descriptor = Reflect.getOwnPropertyDescriptor(target, key);
          if (key !== 'value') return descriptor;
          descriptorReads += 1;
          return { ...descriptor, value: descriptorReads === 1 ? 'captured' : 'changed' };
        },
        get(_target, key) {
          if (key === 'then') return undefined;
          throw new Error('properties must not be read');
        },
      }
    )
  );

  const response = await server.dispatchEntry({ toonrpc: '1.0', method: 'proxy', id: 1 });
  assert.deepEqual(response, { toonrpc: '1.0', result: { value: 'captured' }, id: 1 });
  assert.equal(descriptorReads, 1);
});

test('TOON preflight includes the final root depth and isolates a deep batch response', async () => {
  const server = new Server();
  let boundary = null;
  for (let depth = 0; depth < 999; depth += 1) boundary = { next: boundary };
  server.register('boundary', async () => boundary);
  server.register('ok', async () => 'sibling');

  const single = await server.dispatchEntry({ toonrpc: '1.0', method: 'boundary', id: 1 });
  assert.equal(Object.hasOwn(single, 'result'), true);

  const batch = await handleValue(server, [
    { toonrpc: '1.0', method: 'boundary', id: 1 },
    { toonrpc: '1.0', method: 'ok', id: 2 },
  ]);
  assert.deepEqual(batch[0], {
    toonrpc: '1.0',
    error: { code: -32603, message: 'Internal error' },
    id: 1,
  });
  assert.deepEqual(batch[1], { toonrpc: '1.0', result: 'sibling', id: 2 });
});
