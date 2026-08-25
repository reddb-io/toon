import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dualDialectStream } from '../dist/acp-stream.js';

// Regression suite for the red-skills field report (#416): ndJsonStream
// behavioral parity — batch frames, per-frame error skipping, latch-on-decode,
// cancel, and EOF flush.

function bytePipe() {
  let controller;
  const readable = new ReadableStream({ start(c) { controller = c; } });
  const encoder = new TextEncoder();
  return {
    readable,
    push: (text) => controller.enqueue(encoder.encode(text)),
    end: () => controller.close(),
    fail: (err) => controller.error(err),
  };
}

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

test('a JSON-RPC batch frame passes through instead of wedging the stream', async () => {
  const pipe = bytePipe();
  const stream = dualDialectStream(sink().writable, pipe.readable);

  pipe.push('[{"jsonrpc":"2.0","method":"a","id":1},{"jsonrpc":"2.0","method":"b","id":2}]\n');
  pipe.push('{"jsonrpc":"2.0","method":"after","id":3}\n');

  const [batch, single] = await collect(stream.readable, 2);
  assert.ok(Array.isArray(batch), 'batch arrives as an array');
  assert.deepEqual(batch[0], { jsonrpc: '2.0', method: 'a', id: 1 });
  assert.deepEqual(batch[1], { jsonrpc: '2.0', method: 'b', id: 2 });
  // Nothing queued behind the batch: the next frame arrives normally.
  assert.deepEqual(single, { jsonrpc: '2.0', method: 'after', id: 3 });
});

test('a batch write goes out as one JSON line in either dialect', async () => {
  for (const preferred of ['jsonrpc', 'toonrpc']) {
    const out = sink();
    const stream = dualDialectStream(out.writable, bytePipe().readable, { preferred });
    const writer = stream.writable.getWriter();
    const batch = [
      { jsonrpc: '2.0', result: 1, id: 1 },
      { jsonrpc: '2.0', result: 2, id: 2 },
    ];
    await writer.write(batch);
    writer.releaseLock();
    assert.equal(
      out.text(),
      '[{"jsonrpc":"2.0","result":1,"id":1},{"jsonrpc":"2.0","result":2,"id":2}]\n',
      `dialect ${preferred}: a top-level array is always one JSON line`
    );
  }
});

test('a malformed frame is reported and skipped; the connection survives', async () => {
  const pipe = bytePipe();
  const diagnostics = [];
  const stream = dualDialectStream(sink().writable, pipe.readable, {
    onDiagnostic: (d) => diagnostics.push(d),
  });

  pipe.push('{"jsonrpc":"2.0","method":"broken",\n');
  pipe.push('{"jsonrpc":"2.0","method":"alive","id":2}\n');

  const [message] = await collect(stream.readable, 1);
  assert.deepEqual(message, { jsonrpc: '2.0', method: 'alive', id: 2 });
  const skipped = diagnostics.filter((d) => d.reason === 'skipped-frame');
  assert.equal(skipped.length, 1);
  assert.equal(skipped[0].dialect, 'jsonrpc');
  assert.ok(skipped[0].frame.length <= 200);
});

test('the peer dialect latches on a successful decode, not on the sniff', async () => {
  const pipe = bytePipe();
  const out = sink();
  const diagnostics = [];
  const stream = dualDialectStream(out.writable, pipe.readable, {
    onDiagnostic: (d) => diagnostics.push(d),
  });

  // Garbage that does not start with `{`/`[`: sniffed TOON, fails to decode.
  pipe.push('!!! not a document !!!\n\n');
  pipe.push('{"jsonrpc":"2.0","method":"ping","id":1}\n');
  await collect(stream.readable, 1);

  const writer = stream.writable.getWriter();
  await writer.write({ jsonrpc: '2.0', result: 'pong', id: 1 });
  writer.releaseLock();

  assert.equal(
    out.text(),
    '{"jsonrpc":"2.0","result":"pong","id":1}\n',
    'a garbage frame must not flip a JSON peer to TOON'
  );
});

test('EOF flushes a final unterminated JSON frame', async () => {
  const pipe = bytePipe();
  const stream = dualDialectStream(sink().writable, pipe.readable);
  pipe.push('{"jsonrpc":"2.0","method":"last","id":7}');
  pipe.end();
  const messages = await collect(stream.readable, 1);
  assert.deepEqual(messages[0], { jsonrpc: '2.0', method: 'last', id: 7 });
});

test('EOF flushes a final unterminated TOON frame', async () => {
  const pipe = bytePipe();
  const stream = dualDialectStream(sink().writable, pipe.readable);
  pipe.push('toonrpc: "1.0"\nmethod: last\nid: 8\n');
  pipe.end();
  const messages = await collect(stream.readable, 1);
  assert.deepEqual(messages[0], { jsonrpc: '2.0', method: 'last', id: 8 });
});

test('cancelling the readable cancels the underlying byte reader', async () => {
  let cancelled;
  const readable = new ReadableStream({
    start() {},
    cancel(reason) {
      cancelled = String(reason);
    },
  });
  const stream = dualDialectStream(sink().writable, readable);
  await stream.readable.cancel('connection closed');
  assert.equal(cancelled, 'connection closed');
});

test('an envelope-only message still frames and round-trips under TOON', async () => {
  const pipe = bytePipe();
  const out = sink();
  const stream = dualDialectStream(out.writable, pipe.readable, { preferred: 'toonrpc' });
  const writer = stream.writable.getWriter();
  await writer.write({});
  writer.releaseLock();
  assert.ok(out.text().startsWith('toonrpc:'));
  assert.ok(out.text().endsWith('\n\n'));

  const echo = bytePipe();
  const reread = dualDialectStream(sink().writable, echo.readable);
  echo.push(out.text());
  const [message] = await collect(reread.readable, 1);
  assert.deepEqual(message, { jsonrpc: '2.0' });
});

test('an inbound TOON batch is impossible, but a JSON batch under preferred toonrpc still answers per element', async () => {
  const pipe = bytePipe();
  const out = sink();
  const stream = dualDialectStream(out.writable, pipe.readable, { preferred: 'toonrpc' });
  pipe.push('[{"jsonrpc":"2.0","method":"x","id":1}]\n');
  const [batch] = await collect(stream.readable, 1);
  assert.ok(Array.isArray(batch));
  // The JSON batch proved the peer speaks JSON; the answer follows it.
  const writer = stream.writable.getWriter();
  await writer.write([{ jsonrpc: '2.0', result: 'ok', id: 1 }]);
  writer.releaseLock();
  assert.equal(out.text(), '[{"jsonrpc":"2.0","result":"ok","id":1}]\n');
});
