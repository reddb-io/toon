import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as http from 'node:http';
import { encode } from '@reddb-io/toon';
import { Client, Server } from '../dist/index.js';
import { SseTransport, SseTransportError } from '../dist/sse.js';

/** Loopback SSE endpoint: GET opens the event stream, POST dispatches to the RPC server. */
async function listen(server) {
  const streams = new Set();
  const listener = http.createServer((request, response) => {
    if (request.method === 'GET') {
      response.writeHead(200, { 'Content-Type': 'text/event-stream' });
      response.write(': stream open\n\n');
      streams.add(response);
      request.on('close', () => streams.delete(response));
      return;
    }
    const chunks = [];
    request.on('data', (chunk) => chunks.push(chunk));
    request.on('end', () => {
      void server.handle(new Uint8Array(Buffer.concat(chunks))).then((reply) => {
        if (reply.length > 0) {
          const lines = new TextDecoder().decode(reply).split('\n');
          const event = lines.map((line) => `data: ${line}`).join('\n') + '\n\n';
          for (const stream of streams) stream.write(event);
        }
        response.writeHead(reply.length > 0 ? 202 : 204).end();
      });
    });
  });
  await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  return { listener, url: `http://127.0.0.1:${listener.address().port}/rpc` };
}

function rpcServer() {
  const server = new Server();
  server.register('add', async (params) => params.a + params.b);
  server.register('lines', async () => 'first\nsecond\nthird');
  server.register('note', async () => null);
  return server;
}

test('the Client completes calls and notifications over an SSE loopback', async () => {
  const { listener, url } = await listen(rpcServer());
  const client = new Client(new SseTransport({ url }));
  try {
    assert.equal(await client.call('add', { a: 40, b: 2 }), 42);
    await client.notify('note');
    assert.equal(await client.call('add', { a: 1, b: 1 }), 2);
  } finally {
    await client.close();
    listener.close();
  }
});

test('multi-line documents survive the event framing intact', async () => {
  const { listener, url } = await listen(rpcServer());
  const client = new Client(new SseTransport({ url }));
  try {
    assert.equal(await client.call('lines', {}), 'first\nsecond\nthird');
  } finally {
    await client.close();
    listener.close();
  }
});

test('a non-2xx event-stream response rejects open', async () => {
  const listener = http.createServer((request, response) => {
    response.writeHead(503).end('down');
  });
  await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  const transport = new SseTransport({
    url: `http://127.0.0.1:${listener.address().port}/rpc`,
  });
  try {
    await assert.rejects(
      transport.open(),
      (error) => error instanceof SseTransportError && error.status === 503
    );
  } finally {
    await transport.close();
    listener.close();
  }
});

test('a failed POST rejects the call without killing the stream', async () => {
  const server = rpcServer();
  const { listener, url } = await listen(server);
  let failPosts = true;
  const transport = new SseTransport({
    url,
    fetch: async (target, init) => {
      if (init?.method === 'POST' && failPosts) return new Response('no', { status: 500 });
      return fetch(target, init);
    },
  });
  const client = new Client(transport);
  try {
    await assert.rejects(
      client.call('add', { a: 1, b: 1 }),
      (error) => error instanceof SseTransportError && error.status === 500
    );
    failPosts = false;
    assert.equal(await client.call('add', { a: 2, b: 3 }), 5);
  } finally {
    await client.close();
    listener.close();
  }
});

test('close terminates the receive stream and leaves no hung waiter', async () => {
  const { listener, url } = await listen(rpcServer());
  const transport = new SseTransport({ url });
  await transport.open();
  const consumer = (async () => {
    const seen = [];
    for await (const document of transport.receive()) seen.push(document);
    return seen;
  })();
  await transport.close();
  assert.deepEqual(await consumer, []);
  listener.close();
});
