// One side of the TS ↔ Rust interop matrix (`pnpm test:rpc-interop`), the
// counterpart of crates/reddb-io-toon-rpc-examples/src/interop_peer.rs:
//
//   node peer.mjs serve <transport>      prints `ready <url>` once it listens
//   node peer.mjs call <transport> <url> runs the shared call sequence
//
// For stdio the server serves this process's stdin/stdout and prints nothing,
// and the client's <url> is the server command as a JSON array to spawn.

import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import * as http from 'node:http';
import { WebSocketServer } from 'ws';
import { Client, RpcError, Server } from '../../dist/index.js';
import { HttpTransport } from '../../dist/http.js';
import { SseTransport } from '../../dist/sse.js';
import { StdioTransport } from '../../dist/stdio.js';
import { TcpTransport } from '../../dist/tcp.js';
import { WebSocketTransport } from '../../dist/websocket.js';
import {
  attachWebSocket,
  createHttpHandler,
  createSseHandler,
  serveStdio,
  serveTcp,
} from '../../dist/serve.js';
import { CalculatorClient, registerCalculator } from '../generated/calculator.ts';

function calculator() {
  const server = new Server();
  registerCalculator(server, {
    add: (a, b) => a + b,
    divide(a, b) {
      if (b === 0) throw new RpcError(-32602, 'division by zero');
      return a / b;
    },
    norm: (v) => Math.hypot(v.x, v.y),
    echo: (text) => text,
    stats: (values) => ({
      count: values.length,
      mean: values.length === 0 ? null : values.reduce((sum, value) => sum + value, 0) / values.length,
    }),
  });
  return server;
}

async function listen(listener) {
  const server = http.createServer(listener);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return server.address().port;
}

async function serve(transport) {
  const server = calculator();
  switch (transport) {
    case 'tcp': {
      const handle = await serveTcp(server);
      console.log(`ready tcp://127.0.0.1:${handle.address.port}`);
      return;
    }
    case 'http':
      console.log(`ready http://127.0.0.1:${await listen(createHttpHandler(server))}/rpc`);
      return;
    case 'sse':
      console.log(`ready http://127.0.0.1:${await listen(createSseHandler(server))}/rpc`);
      return;
    case 'ws': {
      const wss = new WebSocketServer({ host: '127.0.0.1', port: 0 });
      await new Promise((resolve) => wss.on('listening', resolve));
      wss.on('connection', (socket) => attachWebSocket(server, socket));
      console.log(`ready ws://127.0.0.1:${wss.address().port}`);
      return;
    }
    case 'stdio':
      await serveStdio(server).done;
      return;
    default:
      throw new Error(`unknown transport ${transport}`);
  }
}

async function call(transport, url) {
  let child;
  let client;
  switch (transport) {
    case 'tcp': {
      const { hostname, port } = new URL(url);
      client = new Client(new TcpTransport({ host: hostname, port: Number(port) }));
      break;
    }
    case 'http':
      client = new Client(new HttpTransport({ url }));
      break;
    case 'ws':
      client = new Client(new WebSocketTransport({ url }));
      break;
    case 'sse':
      client = new Client(new SseTransport({ url: `${url}?session=${randomUUID()}` }));
      break;
    case 'stdio': {
      const [program, ...args] = JSON.parse(url);
      child = spawn(program, args, { stdio: ['pipe', 'pipe', 'inherit'] });
      client = new Client(new StdioTransport({ input: child.stdout, output: child.stdin }));
      break;
    }
    default:
      throw new Error(`unknown transport ${transport}`);
  }
  try {
    await sequence(client);
  } finally {
    await client.close();
    child?.kill();
  }
}

/** The shared call sequence; interop_peer.rs runs the same one. */
async function sequence(client) {
  const calculator = new CalculatorClient(client);
  assert.equal(await calculator.add(2, 3), 5, 'add');
  assert.equal(await calculator.norm({ x: 3, y: 4 }), 5, 'norm');
  const text = 'multi\n\nline: with, delimiters';
  assert.equal(await calculator.echo(text), text, 'echo');
  assert.deepEqual(await calculator.stats([1, 2, 6]), { count: 3, mean: 3 }, 'stats');
  assert.deepEqual(await calculator.stats([]), { count: 0, mean: null }, 'empty stats');
  await assert.rejects(
    calculator.divide(1, 0),
    (error) => error instanceof RpcError && error.code === -32602,
    'divide by zero'
  );
  await client.notify('add', [1, 2]);
  const sums = await Promise.all([...Array(8).keys()].map((n) => client.call('add', [n, 100])));
  assert.deepEqual(sums, [...Array(8).keys()].map((n) => n + 100), 'concurrent add');
}

const [mode, transport, url] = process.argv.slice(2);
try {
  if (mode === 'serve') await serve(transport);
  else if (mode === 'call') await call(transport, url);
  else throw new Error('usage: peer.mjs serve <transport> | call <transport> <url>');
} catch (error) {
  console.error(`peer.mjs: ${error?.stack ?? error}`);
  process.exit(1);
}
