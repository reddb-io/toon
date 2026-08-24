import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as net from 'node:net';
import { PassThrough } from 'node:stream';
import { encode } from '@reddb-io/toon';
import { Client, ClientClosedError, Server } from '../dist/index.js';
import { FrameDecoder, FramingError, encodeFrame } from '../dist/framing.js';
import { TcpTransport } from '../dist/tcp.js';

const bytes = (value) => new TextEncoder().encode(encode(value));

/** Framed loopback RPC server; `mode` shapes how response bytes hit the wire. */
async function listen(server, { mode = 'whole' } = {}) {
  const listener = net.createServer((socket) => {
    const decoder = new FrameDecoder();
    socket.on('data', (chunk) => {
      for (const document of decoder.push(new Uint8Array(chunk))) {
        void server.handle(document).then((reply) => {
          if (reply.length === 0) return;
          const frame = encodeFrame(reply);
          if (mode === 'whole') {
            socket.write(frame);
            return;
          }
          // Dribble the frame byte by byte to exercise reassembly.
          for (const byte of frame) socket.write(new Uint8Array([byte]));
        });
      }
    });
  });
  await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  return { listener, port: listener.address().port };
}

function rpcServer() {
  const server = new Server();
  server.register('add', async (params) => params.a + params.b);
  server.register('note', async () => null);
  return server;
}

test('the Client completes calls and notifications over a TCP loopback', async () => {
  const { listener, port } = await listen(rpcServer());
  const client = new Client(new TcpTransport({ host: '127.0.0.1', port }));
  try {
    assert.equal(await client.call('add', { a: 20, b: 22 }), 42);
    await client.notify('note');
    assert.equal(await client.call('add', { a: 1, b: 2 }), 3);
  } finally {
    await client.close();
    listener.close();
  }
});

test('responses split into single-byte chunks still reassemble', async () => {
  const { listener, port } = await listen(rpcServer(), { mode: 'dribble' });
  const client = new Client(new TcpTransport({ host: '127.0.0.1', port }));
  try {
    const results = await Promise.all([
      client.call('add', { a: 1, b: 1 }),
      client.call('add', { a: 2, b: 2 }),
      client.call('add', { a: 3, b: 3 }),
    ]);
    assert.deepEqual(results, [2, 4, 6]);
  } finally {
    await client.close();
    listener.close();
  }
});

test('a malformed frame from the peer fails the transport, not the process', async () => {
  const listener = net.createServer((socket) => {
    socket.write('not a frame\n');
  });
  await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  const transport = new TcpTransport({ host: '127.0.0.1', port: listener.address().port });
  try {
    await transport.open();
    await assert.rejects(
      (async () => {
        for await (const document of transport.receive()) void document;
      })(),
      FramingError
    );
  } finally {
    await transport.close();
    listener.close();
  }
});

test('remote close mid-call fails the pending call through the Client', async () => {
  const listener = net.createServer((socket) => {
    socket.on('data', () => socket.destroy());
  });
  await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  const client = new Client(new TcpTransport({ host: '127.0.0.1', port: listener.address().port }));
  try {
    await assert.rejects(client.call('add', { a: 1, b: 1 }), ClientClosedError);
  } finally {
    await client.close();
    listener.close();
  }
});

test('a connection refusal rejects open', async () => {
  const probe = net.createServer();
  await new Promise((resolve) => probe.listen(0, '127.0.0.1', resolve));
  const deadPort = probe.address().port;
  await new Promise((resolve) => probe.close(resolve));

  const transport = new TcpTransport({ host: '127.0.0.1', port: deadPort });
  await assert.rejects(transport.open());
  await transport.close();
});

test('an injected duplex stream drives the transport without a network', async () => {
  const wire = new PassThrough();
  wire.connecting = false;
  wire.write = (data, callback) => {
    // Echo each received document back, framed, on the same stream.
    const decoder = new FrameDecoder();
    for (const document of decoder.push(new Uint8Array(data))) {
      wire.push(encodeFrame(document));
    }
    callback?.();
    return true;
  };
  const transport = new TcpTransport({ connect: () => wire });
  await transport.open();
  const received = (async () => {
    for await (const document of transport.receive()) return document;
    return undefined;
  })();
  await transport.send(bytes({ toonrpc: '1.0', result: 'echo', id: 1 }));
  assert.deepEqual(new Uint8Array(await received), bytes({ toonrpc: '1.0', result: 'echo', id: 1 }));
  await transport.close();
});
