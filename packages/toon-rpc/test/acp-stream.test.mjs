import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dualDialectStream } from '../dist/acp-stream.js';

/** A byte pipe: what one side writes, the other side's stream reads. */
function bytePipe() {
  let controller;
  const readable = new ReadableStream({ start(c) { controller = c; } });
  const encoder = new TextEncoder();
  return {
    readable,
    push: (text) => controller.enqueue(encoder.encode(text)),
    end: () => controller.close(),
  };
}

/** Collect everything written to a WritableStream<Uint8Array> as text. */
function sink() {
  const chunks = [];
  const writable = new WritableStream({
    write(chunk) { chunks.push(chunk); },
  });
  return { writable, text: () => chunks.map((c) => new TextDecoder().decode(c)).join('') };
}

async function collect(readable, count) {
  const reader = readable.getReader();
  const out = [];
  while (out.length < count) {
    const { done, value } = await reader.read();
    if (done) break;
    out.push(value);
  }
  reader.releaseLock();
  return out;
}

test('reads a JSON line and a TOON document off the same stream', async () => {
  const pipe = bytePipe();
  const out = sink();
  const stream = dualDialectStream(out.writable, pipe.readable);

  pipe.push('{"jsonrpc":"2.0","method":"ping","id":1}\n');
  pipe.push('toonrpc: "1.0"\nmethod: pong\nid: 2\n\n');

  const messages = await collect(stream.readable, 2);
  assert.deepEqual(messages[0], { jsonrpc: '2.0', method: 'ping', id: 1 });
  // The consumer sees a jsonrpc envelope regardless of the wire dialect.
  assert.deepEqual(messages[1], { jsonrpc: '2.0', method: 'pong', id: 2 });
});

test('a frame split across chunks is reassembled', async () => {
  const pipe = bytePipe();
  const stream = dualDialectStream(sink().writable, pipe.readable);

  pipe.push('toonrpc: "1.0"\nmeth');
  pipe.push('od: ping\nid: 9\n');
  pipe.push('\n');

  const [message] = await collect(stream.readable, 1);
  assert.deepEqual(message, { jsonrpc: '2.0', method: 'ping', id: 9 });
});

test('writes JSON by default, before the peer has proven anything', async () => {
  const pipe = bytePipe();
  const out = sink();
  const stream = dualDialectStream(out.writable, pipe.readable);

  const writer = stream.writable.getWriter();
  await writer.write({ jsonrpc: '2.0', method: 'hello', id: 1 });
  writer.releaseLock();

  assert.equal(out.text(), '{"jsonrpc":"2.0","method":"hello","id":1}\n');
});

test('answers in kind: a TOON peer gets TOON back', async () => {
  const pipe = bytePipe();
  const out = sink();
  const stream = dualDialectStream(out.writable, pipe.readable);

  pipe.push('toonrpc: "1.0"\nmethod: ping\nid: 1\n\n');
  await collect(stream.readable, 1);

  const writer = stream.writable.getWriter();
  await writer.write({ jsonrpc: '2.0', result: 'pong', id: 1 });
  writer.releaseLock();

  const written = out.text();
  assert.ok(written.startsWith('toonrpc:'), `expected a TOON frame, got: ${written}`);
  assert.ok(written.endsWith('\n\n'), 'TOON frames terminate with a blank line');
  assert.ok(written.includes('result: pong'));
});

test('preferred: "toonrpc" opens in TOON', async () => {
  const pipe = bytePipe();
  const out = sink();
  const stream = dualDialectStream(out.writable, pipe.readable, { preferred: 'toonrpc' });

  const writer = stream.writable.getWriter();
  await writer.write({ jsonrpc: '2.0', method: 'hello', id: 1 });
  writer.releaseLock();

  assert.ok(out.text().startsWith('toonrpc:'));
});

test('a JSON peer keeps getting JSON even under preferred: "toonrpc"', async () => {
  const pipe = bytePipe();
  const out = sink();
  const stream = dualDialectStream(out.writable, pipe.readable, { preferred: 'toonrpc' });

  pipe.push('{"jsonrpc":"2.0","method":"ping","id":1}\n');
  await collect(stream.readable, 1);

  const writer = stream.writable.getWriter();
  await writer.write({ jsonrpc: '2.0', result: 'pong', id: 1 });
  writer.releaseLock();

  assert.equal(out.text(), '{"jsonrpc":"2.0","result":"pong","id":1}\n');
});

test('string payloads with embedded newlines survive the TOON framing', async () => {
  const pipe = bytePipe();
  const out = sink();
  const stream = dualDialectStream(out.writable, pipe.readable, { preferred: 'toonrpc' });

  const writer = stream.writable.getWriter();
  const text = 'line one\n\nline two';
  await writer.write({ jsonrpc: '2.0', method: 'say', params: { text }, id: 1 });
  writer.releaseLock();

  // Feed the written bytes back through a reader to prove round-trip.
  const echo = bytePipe();
  const reread = dualDialectStream(sink().writable, echo.readable);
  echo.push(out.text());
  const [message] = await collect(reread.readable, 1);
  assert.equal(message.params.text, text);
});
