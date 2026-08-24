import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PassThrough } from 'node:stream';
import { encode } from '@reddb-io/toon';
import { Client, Server } from '../dist/index.js';
import { FrameDecoder, FramingError, encodeFrame } from '../dist/framing.js';
import { StdioTransport } from '../dist/stdio.js';

const bytes = (value) => new TextEncoder().encode(encode(value));

test('the Client completes calls over framed stdio pipes', async () => {
  const server = new Server();
  server.register('greet', async (params) => `hello ${params.name}`);

  const input = new PassThrough();
  const output = new PassThrough();
  const decoder = new FrameDecoder();
  output.on('data', (chunk) => {
    for (const document of decoder.push(new Uint8Array(chunk))) {
      void server.handle(document).then((reply) => {
        if (reply.length > 0) input.write(encodeFrame(reply));
      });
    }
  });

  const client = new Client(new StdioTransport({ input, output }));
  assert.equal(await client.call('greet', { name: 'toon' }), 'hello toon');
  await client.close();
});

test('documents split across arbitrary stdin chunks reassemble', async () => {
  const input = new PassThrough();
  const transport = new StdioTransport({ input, output: new PassThrough() });
  const received = (async () => {
    for await (const document of transport.receive()) return document;
    return undefined;
  })();
  const frame = encodeFrame(bytes({ toonrpc: '1.0', result: 7, id: 3 }));
  for (const byte of frame) input.write(new Uint8Array([byte]));
  assert.deepEqual(new Uint8Array(await received), bytes({ toonrpc: '1.0', result: 7, id: 3 }));
  await transport.close();
});

test('stdin ending mid-frame fails the receive stream', async () => {
  const input = new PassThrough();
  const transport = new StdioTransport({ input, output: new PassThrough() });
  const consumer = (async () => {
    for await (const document of transport.receive()) void document;
  })();
  input.write('7\npart');
  input.end();
  await assert.rejects(consumer, FramingError);
});

test('stdin ending on a frame boundary ends the receive stream cleanly', async () => {
  const input = new PassThrough();
  const transport = new StdioTransport({ input, output: new PassThrough() });
  const documents = [];
  const consumer = (async () => {
    for await (const document of transport.receive()) documents.push(document);
  })();
  input.write(encodeFrame(bytes({ toonrpc: '1.0', result: 1, id: 1 })));
  input.end();
  await consumer;
  assert.equal(documents.length, 1);
  await transport.close();
});

test('send after close rejects', async () => {
  const transport = new StdioTransport({ input: new PassThrough(), output: new PassThrough() });
  await transport.close();
  await assert.rejects(transport.send(bytes({ toonrpc: '1.0', method: 'x' })), /closed/);
});
