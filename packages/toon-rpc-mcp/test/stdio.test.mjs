/**
 * End-to-end stdio transport: a scripted client transcript driven through
 * `serveStdioWith`, exactly as an MCP client would.
 */

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Readable, Writable } from 'node:stream';
import { createMcpDispatcher, serveStdioWith } from '../dist/index.js';
import { fixtureService } from './fixture.mjs';

/** Feed a transcript through the transport and collect the response lines. */
async function run(transcript, options) {
  const dispatcher = createMcpDispatcher(fixtureService, options);
  const input = Readable.from([transcript]);

  let written = '';
  const output = new Writable({
    write(chunk, _encoding, callback) {
      written += chunk.toString('utf8');
      callback();
    },
  });

  await serveStdioWith(dispatcher, { input, output });

  if (written === '') return { lines: [], raw: '' };
  assert.ok(written.endsWith('\n'), 'every message must be newline-terminated');
  return { lines: written.trimEnd().split('\n').map((l) => JSON.parse(l)), raw: written };
}

/**
 * The transcript a modern MCP client produces on a fresh connection: discover,
 * then list, then call. There is no handshake in this revision.
 */
const MODERN_TRANSCRIPT =
  '{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"ExampleClient","version":"1.0.0"},"io.modelcontextprotocol/clientCapabilities":{}}}}\n' +
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}\n' +
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hello"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}\n';

test('a modern client transcript is served end to end', async () => {
  const { lines } = await run(MODERN_TRANSCRIPT);
  assert.equal(lines.length, 3, 'one response per request');

  assert.equal(lines[0].id, 1);
  assert.equal(lines[0].result.supportedVersions[0], '2026-07-28');

  assert.equal(lines[1].id, 2);
  assert.equal(lines[1].result.tools[0].name, 'echo');
  assert.equal(typeof lines[1].result.tools[0].inputSchema, 'object');

  assert.equal(lines[2].id, 3);
  assert.equal(lines[2].result.content[0].text, 'hello');
  assert.equal(lines[2].result.isError, undefined);
});

test('every response is exactly one line', async () => {
  const { raw } = await run(MODERN_TRANSCRIPT);
  assert.equal(
    raw.split('\n').length - 1,
    3,
    `three messages means exactly three newlines: ${raw}`
  );
});

test('a legacy client transcript fails deterministically against a modern server', async () => {
  const { lines } = await run(
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"LegacyClient","version":"1.0.0"}}}\n'
  );
  assert.equal(lines.length, 1);
  assert.equal(lines[0].error.code, -32601);
  assert.equal(lines[0].error.data.supported[0], '2026-07-28');
});

test('a dual-era transcript completes the legacy handshake', async () => {
  const { lines } = await run(
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}\n' +
      '{"jsonrpc":"2.0","method":"notifications/initialized"}\n' +
      '{"jsonrpc":"2.0","id":2,"method":"tools/list"}\n' +
      '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hi"}}}\n',
    { legacyInitialize: true }
  );

  assert.equal(lines.length, 3, 'the notification must draw no reply');
  assert.equal(lines[0].result.protocolVersion, '2025-11-25');
  assert.equal(lines[1].id, 2);
  assert.equal(lines[2].result.content[0].text, 'hi');
});

test('notifications produce no output at all', async () => {
  const { lines } = await run(
    '{"jsonrpc":"2.0","method":"notifications/initialized"}\n' +
      '{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}\n'
  );
  assert.deepEqual(lines, []);
});

test('a malformed line does not desynchronize the stream', async () => {
  const { lines } = await run(
    '{"jsonrpc":"2.0","id":1,"method":"ping"}\n' +
      '{ this is not json\n' +
      '{"jsonrpc":"2.0","id":3,"method":"ping"}\n'
  );

  assert.equal(lines.length, 3);
  assert.equal(lines[0].id, 1);
  assert.equal(lines[1].error.code, -32700);
  assert.equal(lines[2].id, 3);
  assert.deepEqual(lines[2].result, {});
});

test('responses keep request order even when a call resolves late', async () => {
  // A slow first tool call must not let the second response overtake it.
  const slow = {
    ...fixtureService,
    callTool: async (_name, args) => {
      if (args.text === 'slow') await new Promise((r) => setTimeout(r, 20));
      return { resultType: 'complete', content: [{ type: 'text', text: String(args.text) }] };
    },
  };
  const dispatcher = createMcpDispatcher(slow);
  const input = Readable.from([
    '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"text":"slow"}}}\n' +
      '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"text":"fast"}}}\n',
  ]);

  let written = '';
  const output = new Writable({
    write(chunk, _e, cb) {
      written += chunk.toString('utf8');
      cb();
    },
  });

  await serveStdioWith(dispatcher, { input, output });
  const ids = written.trimEnd().split('\n').map((l) => JSON.parse(l).id);
  assert.deepEqual(ids, [1, 2]);
});

test('blank lines between messages are tolerated', async () => {
  const { lines } = await run(
    '{"jsonrpc":"2.0","id":1,"method":"ping"}\n\n\n{"jsonrpc":"2.0","id":2,"method":"ping"}\n'
  );
  assert.equal(lines.length, 2, 'blank lines are not messages');
});

test('a final line without a trailing newline is still processed', async () => {
  const { lines } = await run('{"jsonrpc":"2.0","id":1,"method":"ping"}');
  assert.equal(lines.length, 1);
});

test('end of input resolves cleanly with no output', async () => {
  const { lines } = await run('');
  assert.deepEqual(lines, []);
});
