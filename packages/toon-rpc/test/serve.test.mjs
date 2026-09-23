import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as http from 'node:http';
import * as net from 'node:net';
import { PassThrough } from 'node:stream';
import { WebSocketServer } from 'ws';
import { decode, encode } from '@reddb-io/toon';
import { Client, Server, encodeFrame } from '../dist/index.js';
import { HttpTransport } from '../dist/http.js';
import { SseTransport } from '../dist/sse.js';
import { StdioTransport } from '../dist/stdio.js';
import { TcpTransport } from '../dist/tcp.js';
import { WebSocketTransport } from '../dist/websocket.js';
import {
  attachWebSocket,
  createHttpHandler,
  createSseHandler,
  serveStdio,
  serveTcp,
} from '../dist/serve.js';

const text = 'multi\n\nline';

function echoServer() {
  const server = new Server();
  server.register('echo', async (params) => (Array.isArray(params) ? params[0] : null));
  return server;
}

async function roundTrip(client) {
  const results = await Promise.all(
    [0, 1, 2, 3].map((n) => client.call('echo', [`${text} ${n}`]))
  );
  assert.deepEqual(results, [0, 1, 2, 3].map((n) => `${text} ${n}`));
  await client.notify('echo', ['ignored']);
}

async function listen(listener) {
  const server = http.createServer(listener);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return server;
}

test('TCP: framed calls, an oversized frame closes, close drains', async () => {
  const handle = await serveTcp(echoServer(), { limits: { maxFrameBytes: 1024 } });
  const client = new Client(new TcpTransport({ host: '127.0.0.1', port: handle.address.port }));
  await roundTrip(client);

  const raw = net.createConnection(handle.address.port, '127.0.0.1');
  raw.write('4096\n');
  await new Promise((resolve) => raw.on('close', resolve));

  const pending = client.call('echo', ['last']);
  assert.equal(await pending, 'last');
  await handle.close();
  await assert.rejects(client.call('echo', ['after close']));
  await client.close();
});

test('stdio: framed calls over a pair of pipes', async () => {
  const toServer = new PassThrough();
  const toClient = new PassThrough();
  const handle = serveStdio(echoServer(), { input: toServer, output: toClient });
  const client = new Client(new StdioTransport({ input: toClient, output: toServer }));
  await roundTrip(client);
  toServer.end();
  await handle.done;
  await client.close();
});

test('stdio: a notification gets no frame back', async () => {
  const toServer = new PassThrough();
  const toClient = new PassThrough();
  const handle = serveStdio(echoServer(), { input: toServer, output: toClient });
  const received = [];
  toClient.on('data', (chunk) => received.push(chunk));
  toServer.end(encodeFrame(new TextEncoder().encode(encode({ toonrpc: '1.0', method: 'echo' }))));
  await handle.done;
  assert.deepEqual(received, []);
});

test('HTTP: 200 with TOON, 204 for notifications, 405 and 413', async () => {
  const server = await listen(createHttpHandler(echoServer(), { limits: { maxBodyBytes: 256 } }));
  const url = `http://127.0.0.1:${server.address().port}/rpc`;
  await roundTrip(new Client(new HttpTransport({ url })));

  const notification = await fetch(url, {
    method: 'POST',
    body: encode({ toonrpc: '1.0', method: 'echo' }),
  });
  assert.equal(notification.status, 204);
  assert.equal((await fetch(url)).status, 405);
  const big = await fetch(url, { method: 'POST', body: 'x'.repeat(1024) });
  assert.equal(big.status, 413);
  server.close();
});

test('SSE: responses arrive on the session stream; closeSessions ends it', async () => {
  const handler = createSseHandler(echoServer());
  const server = await listen(handler);
  const url = `http://127.0.0.1:${server.address().port}/rpc?session=s1`;
  const client = new Client(new SseTransport({ url }));
  await roundTrip(client);
  assert.equal(handler.sessionCount, 1);

  const post = await fetch(url, {
    method: 'POST',
    body: encode({ toonrpc: '1.0', method: 'echo', params: ['x'], id: 'raw' }),
  });
  assert.equal(post.status, 202);
  assert.equal(await post.text(), '');
  assert.equal((await fetch(`${url}x`, { method: 'POST', body: 'x' })).status, 404);
  assert.equal((await fetch(url.replace('?session=s1', ''))).status, 400);

  handler.closeSessions();
  await assert.rejects(client.call('echo', ['after close']));
  await client.close();
  server.close();
});

test('WebSocket: text and binary answers, nothing for notifications, 1009 past the limit', async () => {
  const wss = new WebSocketServer({ port: 0, host: '127.0.0.1', maxPayload: 4096 });
  await new Promise((resolve) => wss.on('listening', resolve));
  wss.on('connection', (socket) =>
    attachWebSocket(echoServer(), socket, { limits: { maxFrameBytes: 1024 } })
  );
  const url = `ws://127.0.0.1:${wss.address().port}`;
  const client = new Client(new WebSocketTransport({ url }));
  await roundTrip(client);
  await client.close();

  const raw = new WebSocket(url);
  raw.binaryType = 'arraybuffer';
  await new Promise((resolve) => raw.addEventListener('open', resolve));
  const replies = [];
  raw.addEventListener('message', (event) => replies.push(event.data));
  raw.send(encode({ toonrpc: '1.0', method: 'echo' }));
  raw.send(new TextEncoder().encode(encode({ toonrpc: '1.0', method: 'echo', params: [1], id: 1 })));
  raw.send(encode({ toonrpc: '1.0', method: 'echo', params: ['t'], id: 2 }));
  while (replies.length < 2) await new Promise((resolve) => setTimeout(resolve, 5));
  assert.ok(replies[0] instanceof ArrayBuffer);
  assert.equal(decode(new TextDecoder().decode(replies[0])).id, 1);
  assert.equal(typeof replies[1], 'string');
  assert.equal(decode(replies[1]).id, 2);

  raw.send('x'.repeat(2048));
  const closed = await new Promise((resolve) => raw.addEventListener('close', resolve));
  assert.equal(closed.code, 1009);
  await new Promise((resolve) => wss.close(resolve));
});
