import assert from 'node:assert/strict';
import { test } from 'node:test';
import { decode, encode } from '@reddb-io/toon';
import { Client, Server } from '../dist/index.js';
import { HttpTransport, HttpTransportError } from '../dist/http.js';

const bytes = (value) => new TextEncoder().encode(encode(value));
const value = (document) => decode(new TextDecoder('utf8', { fatal: true }).decode(document));

function fetchDouble(handler) {
  const calls = [];
  const impl = async (url, init) => {
    calls.push({ url: String(url), init });
    if (init.signal?.aborted) throw Object.assign(new Error('aborted'), { name: 'AbortError' });
    return handler(init, calls.length);
  };
  return { impl, calls };
}

function response(body, init = {}) {
  return new Response(body, { status: 200, ...init });
}

test('request maps one POST to one response document', async () => {
  const { impl, calls } = fetchDouble(async (init) =>
    response(bytes({ toonrpc: '1.0', result: value(init.body), id: 1 }))
  );
  const transport = new HttpTransport({ url: 'http://rpc.test/rpc', fetch: impl });
  const reply = await transport.request(bytes({ toonrpc: '1.0', method: 'echo', id: 1 }));
  assert.deepEqual(value(reply), {
    toonrpc: '1.0',
    result: { toonrpc: '1.0', method: 'echo', id: 1 },
    id: 1,
  });
  assert.equal(calls[0].init.method, 'POST');
  assert.equal(calls[0].init.headers['Content-Type'], 'application/toon');
  assert.equal(calls[0].init.headers.Accept, 'application/toon');
});

test('204 and empty bodies map to no response document', async () => {
  const noContent = new HttpTransport({
    url: 'http://rpc.test/rpc',
    fetch: fetchDouble(async () => new Response(null, { status: 204 })).impl,
  });
  assert.equal(await noContent.request(bytes({ toonrpc: '1.0', method: 'ping' })), undefined);

  const emptyBody = new HttpTransport({
    url: 'http://rpc.test/rpc',
    fetch: fetchDouble(async () => response(new Uint8Array(0))).impl,
  });
  assert.equal(await emptyBody.request(bytes({ toonrpc: '1.0', method: 'ping' })), undefined);
});

test('a non-2xx status rejects with the status attached', async () => {
  const transport = new HttpTransport({
    url: 'http://rpc.test/rpc',
    fetch: fetchDouble(async () => new Response('nope', { status: 502, statusText: 'Bad Gateway' }))
      .impl,
  });
  await assert.rejects(
    transport.request(bytes({ toonrpc: '1.0', method: 'x', id: 1 })),
    (error) => error instanceof HttpTransportError && error.status === 502
  );
});

test('an aborted signal rejects before any fetch happens', async () => {
  const { impl, calls } = fetchDouble(async () => response(new Uint8Array(0)));
  const transport = new HttpTransport({ url: 'http://rpc.test/rpc', fetch: impl });
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(
    transport.request(bytes({ toonrpc: '1.0', method: 'x', id: 1 }), {
      signal: controller.signal,
    })
  );
  assert.equal(calls.length, 0);
});

test('close aborts the transport for later requests', async () => {
  const { impl, calls } = fetchDouble(async () => response(new Uint8Array(0)));
  const transport = new HttpTransport({ url: 'http://rpc.test/rpc', fetch: impl });
  await transport.close();
  await assert.rejects(transport.request(bytes({ toonrpc: '1.0', method: 'x', id: 1 })));
  assert.equal(calls.length, 0);
});

test('the Client completes calls and notifications over HTTP end-to-end', async () => {
  const server = new Server();
  server.register('add', async (params) => params.a + params.b);
  const notified = [];
  server.register('note', async (params) => {
    notified.push(params);
    return null;
  });

  const transport = new HttpTransport({
    url: 'http://rpc.test/rpc',
    fetch: fetchDouble(async (init) => {
      const reply = await server.handle(new Uint8Array(init.body));
      return reply.length === 0 ? new Response(null, { status: 204 }) : response(reply);
    }).impl,
  });
  const client = new Client(transport);
  assert.equal(await client.call('add', { a: 2, b: 3 }), 5);
  await client.notify('note', { seen: true });
  assert.deepEqual(notified, [{ seen: true }]);
  await client.close();
});

test('a response document for a different call terminates the exchange with a protocol error', async () => {
  const transport = new HttpTransport({
    url: 'http://rpc.test/rpc',
    fetch: fetchDouble(async () => response(bytes({ toonrpc: '1.0', result: 1, id: 'other' })))
      .impl,
  });
  const client = new Client(transport);
  await assert.rejects(client.call('add', {}, { id: 'mine' }), /matching response/);
  await client.close();
});
