import assert from 'node:assert/strict';
import { test } from 'node:test';
import { decode, encode } from '@reddb-io/toon';
import {
  Client,
  ClientLimitError,
  ClientTimeoutError,
  DEFAULT_LIMITS,
  FrameDecoder,
  Server,
  encodeFrame,
} from '../dist/index.js';
import { DocumentQueue } from '../dist/internal.js';

const bytes = (value) => new TextEncoder().encode(encode(value));

test('defaults match the Rust limits', () => {
  assert.deepEqual(
    { ...DEFAULT_LIMITS },
    {
      maxFrameBytes: 16 * 1024 * 1024,
      maxBodyBytes: 16 * 1024 * 1024,
      maxBatchLength: 1024,
      maxPendingCalls: 1024,
      maxConnections: 1024,
      maxQueuedDocuments: 1024,
      idleTimeoutMs: 300_000,
      requestTimeoutMs: undefined,
      shutdownGraceMs: 10_000,
    }
  );
});

test('a frame over the limit fails the decoder before its payload arrives', () => {
  const decoder = new FrameDecoder({ maxFrameBytes: 4 });
  assert.equal(decoder.push(encodeFrame(new Uint8Array(4))).length, 1);
  assert.throws(() => decoder.push(new TextEncoder().encode('5\n')), /exceeds the size limit/);
  assert.throws(() => decoder.push(encodeFrame(new Uint8Array(1))), /exceeds the size limit/);
});

test('a full receive queue fails the stream instead of growing', async () => {
  const queue = new DocumentQueue(2);
  for (const n of [1, 2, 3]) queue.push(new Uint8Array([n]));
  const received = [];
  await assert.rejects(async () => {
    for await (const document of queue.iterate()) received.push(document[0]);
  }, /receive queue overflow/);
  assert.deepEqual(received, [1, 2]);
});

test('a batch over the limit is one Invalid Request', async () => {
  const server = new Server({ maxBatchLength: 2 });
  server.register('m', async () => 1);
  const request = (id) => ({ toonrpc: '1.0', method: 'm', id });
  assert.equal(decode(new TextDecoder().decode(await server.handle(bytes([request(0), request(1)])))).length, 2);
  const over = decode(
    new TextDecoder().decode(await server.handle(bytes([request(0), request(1), request(2)])))
  );
  assert.deepEqual(over, {
    toonrpc: '1.0',
    error: { code: -32600, message: 'Invalid Request: batch too large' },
    id: null,
  });
});

test('the client refuses calls past its pending cap and applies its default timeout', async () => {
  const transport = new SilentTransport();
  const client = new Client(transport, { maxPendingCalls: 2, requestTimeoutMs: 20 });
  const first = client.call('a');
  const second = client.call('b');
  await assert.rejects(client.call('c'), ClientLimitError);
  await assert.rejects(first, ClientTimeoutError);
  await assert.rejects(second, ClientTimeoutError);
  assert.equal(client.pendingCallCount, 0);
  await client.close();
});

class SilentTransport {
  kind = 'duplex';
  #ended;
  #end = new Promise((resolve) => (this.#ended = resolve));

  async send() {}

  async *receive() {
    await this.#end;
  }

  async close() {
    this.#ended();
  }
}
