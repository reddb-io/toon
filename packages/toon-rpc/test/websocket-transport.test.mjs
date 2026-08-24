import assert from 'node:assert/strict';
import { test } from 'node:test';
import { decode, encode } from '@reddb-io/toon';
import { Client, ClientClosedError, Server } from '../dist/index.js';
import { WebSocketTransport } from '../dist/websocket.js';

const bytes = (value) => new TextEncoder().encode(encode(value));
const text = (document) => new TextDecoder('utf8', { fatal: true }).decode(document);

/** Scriptable double implementing the addEventListener surface the transport uses. */
class FakeWebSocket {
  static instances = [];
  constructor(url) {
    this.url = url;
    this.readyState = 0;
    this.binaryType = 'blob';
    this.sent = [];
    this.listeners = new Map();
    FakeWebSocket.instances.push(this);
  }
  addEventListener(type, listener) {
    const bucket = this.listeners.get(type) ?? [];
    bucket.push(listener);
    this.listeners.set(type, bucket);
  }
  emit(type, event = {}) {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }
  send(data) {
    this.sent.push(data);
  }
  close() {
    if (this.readyState === 3) return;
    this.readyState = 3;
    queueMicrotask(() => this.emit('close', { code: 1000 }));
  }
  acceptConnection() {
    this.readyState = 1;
    this.emit('open', {});
  }
}

function newTransport() {
  const transport = new WebSocketTransport({ url: 'ws://rpc.test', webSocket: FakeWebSocket });
  const opening = transport.open();
  const socket = FakeWebSocket.instances.at(-1);
  socket.acceptConnection();
  return { transport, socket, opening };
}

test('open resolves on the open event and send writes one frame per document', async () => {
  const { transport, socket, opening } = newTransport();
  await opening;
  await transport.send(bytes({ toonrpc: '1.0', method: 'ping', id: 1 }));
  assert.equal(socket.sent.length, 1);
  assert.equal(socket.binaryType, 'arraybuffer');
  await transport.close();
});

test('binary and text frames each deliver one complete document', async () => {
  const { transport, socket, opening } = newTransport();
  await opening;
  const received = [];
  const consumer = (async () => {
    for await (const document of transport.receive()) received.push(text(document));
  })();
  socket.emit('message', { data: bytes({ toonrpc: '1.0', result: 1, id: 1 }).buffer });
  socket.emit('message', { data: encode({ toonrpc: '1.0', result: 2, id: 2 }) });
  await transport.close();
  await consumer;
  assert.equal(received.length, 2);
  assert.deepEqual(decode(received[0]), { toonrpc: '1.0', result: 1, id: 1 });
  assert.deepEqual(decode(received[1]), { toonrpc: '1.0', result: 2, id: 2 });
});

test('an unsupported frame payload fails the receive stream deterministically', async () => {
  const { transport, socket, opening } = newTransport();
  await opening;
  const consumer = (async () => {
    const seen = [];
    for await (const document of transport.receive()) seen.push(document);
    return seen;
  })();
  socket.emit('message', { data: { not: 'a frame' } });
  await assert.rejects(consumer, /unsupported frame payload/);
  assert.equal(socket.readyState, 3);
});

test('remote close ends the receive stream cleanly', async () => {
  const { transport, socket, opening } = newTransport();
  await opening;
  const consumer = (async () => {
    const seen = [];
    for await (const document of transport.receive()) seen.push(document);
    return seen;
  })();
  socket.close();
  assert.deepEqual(await consumer, []);
  await transport.close();
});

test('a connection error before open rejects open', async () => {
  const transport = new WebSocketTransport({ url: 'ws://rpc.test', webSocket: FakeWebSocket });
  const opening = transport.open();
  FakeWebSocket.instances.at(-1).emit('error', { message: 'refused' });
  await assert.rejects(opening, /refused/);
});

test('send after close rejects and repeated close is idempotent', async () => {
  const { transport, opening } = newTransport();
  await opening;
  await transport.close();
  await transport.close();
  await assert.rejects(transport.send(bytes({ toonrpc: '1.0', method: 'x' })));
});

test('the Client completes concurrent calls over a WebSocket loopback', async () => {
  const server = new Server();
  server.register('double', async (params) => params.n * 2);

  const { transport, socket, opening } = newTransport();
  await opening;
  socket.send = (data) => {
    void server
      .handle(typeof data === 'string' ? new TextEncoder().encode(data) : new Uint8Array(data))
      .then((reply) => {
        if (reply.length > 0) socket.emit('message', { data: reply.buffer });
      });
  };

  const client = new Client(transport);
  const [a, b] = await Promise.all([
    client.call('double', { n: 2 }),
    client.call('double', { n: 21 }),
  ]);
  assert.equal(a, 4);
  assert.equal(b, 42);
  await client.close();
});

test('remote close fails in-flight calls through the Client lifecycle', async () => {
  const { transport, socket, opening } = newTransport();
  await opening;
  const client = new Client(transport);
  const call = client.call('never', {});
  await client.start();
  while (socket.sent.length === 0) await new Promise((resolve) => setImmediate(resolve));
  socket.close();
  await assert.rejects(call, ClientClosedError);
  await client.close();
});
