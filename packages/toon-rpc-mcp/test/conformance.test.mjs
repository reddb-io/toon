/**
 * Conformance of the wire shapes against MCP revision 2026-07-28.
 *
 * Every expected value is transcribed from the official specification pages for
 * the pinned revision. See `docs/mcp-conformance.md` for the citations.
 */

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MCP_PROTOCOL_VERSION, createMcpDispatcher } from '../dist/index.js';
import { fixtureService } from './fixture.mjs';

const dispatcher = () => createMcpDispatcher(fixtureService);

async function call(line) {
  const raw = await dispatcher().handleLine(line);
  assert.notEqual(raw, null, 'a request must produce a response');
  assert.ok(!raw.includes('\n'), `a stdio message must not contain an embedded newline: ${raw}`);
  return JSON.parse(raw);
}

async function resultOf(line) {
  const response = await call(line);
  assert.equal(response.jsonrpc, '2.0');
  assert.equal(response.error, undefined, `expected a result, got ${JSON.stringify(response)}`);
  return response.result;
}

async function errorOf(line) {
  const response = await call(line);
  assert.equal(response.jsonrpc, '2.0');
  assert.equal(response.result, undefined, `expected an error, got ${JSON.stringify(response)}`);
  return response.error;
}

test('the pinned protocol version is the revision this package implements', () => {
  assert.equal(MCP_PROTOCOL_VERSION, '2026-07-28');
});

test('a response carries exactly one of result or error', async () => {
  const ok = await call('{"jsonrpc":"2.0","id":1,"method":"ping"}');
  assert.ok('result' in ok && !('error' in ok));

  const err = await call('{"jsonrpc":"2.0","id":1,"method":"no/such"}');
  assert.ok('error' in err && !('result' in err));
});

// --- server/discover -------------------------------------------------------

test('server/discover matches the schema shape', async () => {
  const result = await resultOf(
    '{"jsonrpc":"2.0","id":"discover-1","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"ExampleClient","version":"1.0.0"},"io.modelcontextprotocol/clientCapabilities":{}}}}'
  );

  assert.equal(result.resultType, 'complete');
  assert.deepEqual(result.supportedVersions, ['2026-07-28']);
  assert.deepEqual(result._meta['io.modelcontextprotocol/serverInfo'], {
    name: 'fixture-server',
    version: '1.0.0',
  });
  assert.ok(result.capabilities.tools);
  assert.ok(result.capabilities.resources);
  assert.ok(result.capabilities.prompts);
  assert.equal(typeof result.instructions, 'string');
});

test('server/discover is answered without any prior handshake', async () => {
  const result = await resultOf('{"jsonrpc":"2.0","id":1,"method":"server/discover"}');
  assert.equal(result.resultType, 'complete');
});

// --- tools -----------------------------------------------------------------

test('tools/list uses the tools key and camelCase inputSchema', async () => {
  const result = await resultOf('{"jsonrpc":"2.0","id":1,"method":"tools/list"}');

  assert.equal(result.resultType, 'complete');
  assert.equal(result.items, undefined, 'the list key is "tools", never "items"');
  assert.equal(result.tools.length, 1);
  assert.equal(result.tools[0].name, 'echo');
  assert.equal(typeof result.tools[0].inputSchema, 'object');
  assert.equal(result.tools[0].inputSchema.type, 'object');
  assert.equal(result.tools[0].input_schema, undefined);
});

test('tools/call returns content blocks', async () => {
  const result = await resultOf(
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hi"}}}'
  );
  assert.equal(result.resultType, 'complete');
  assert.deepEqual(result.content, [{ type: 'text', text: 'hi' }]);
  assert.equal(result.isError, undefined, 'isError is omitted on success, never null');
});

test('a tool execution failure is a result with isError, not a JSON-RPC error', async () => {
  const result = await resultOf(
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{}}}'
  );
  assert.equal(result.isError, true);
  assert.equal(result.content[0].type, 'text');
});

test('an unknown tool is a protocol error with code -32602', async () => {
  const error = await errorOf(
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope"}}'
  );
  assert.equal(error.code, -32602);
  assert.match(error.message, /Unknown tool/);
});

test('tools/call without a name is invalid params', async () => {
  const error = await errorOf('{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{}}');
  assert.equal(error.code, -32602);
});

// --- resources -------------------------------------------------------------

test('resources/list uses the resources key and camelCase mimeType', async () => {
  const result = await resultOf('{"jsonrpc":"2.0","id":1,"method":"resources/list"}');

  assert.equal(result.items, undefined);
  assert.equal(result.resources[0].uri, 'file:///fixture/readme.md');
  assert.equal(result.resources[0].mimeType, 'text/markdown');
  assert.equal(result.resources[0].mime_type, undefined, 'mime_type is not a schema key');
});

test('resources/read wraps entries in contents', async () => {
  const result = await resultOf(
    '{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"file:///fixture/readme.md"}}'
  );
  assert.equal(result.resultType, 'complete');
  assert.equal(result.contents[0].uri, 'file:///fixture/readme.md');
  assert.equal(result.contents[0].mimeType, 'text/markdown');
  assert.equal(result.contents[0].text, '# Fixture');
});

test('a missing resource is -32602 and never an empty contents array', async () => {
  const error = await errorOf(
    '{"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"file:///nonexistent.txt"}}'
  );
  assert.equal(error.code, -32602);
  assert.equal(error.message, 'Resource not found');
  assert.equal(error.data.uri, 'file:///nonexistent.txt');
});

// --- prompts ---------------------------------------------------------------

test('prompts/list uses the prompts key', async () => {
  const result = await resultOf('{"jsonrpc":"2.0","id":1,"method":"prompts/list"}');
  assert.equal(result.items, undefined);
  assert.equal(result.prompts[0].name, 'greet');
});

test('prompts/get returns messages', async () => {
  const result = await resultOf(
    '{"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"greet","arguments":{"who":"Ada"}}}'
  );
  assert.deepEqual(result.messages, [
    { role: 'user', content: { type: 'text', text: 'Hello, Ada!' } },
  ]);
});

test('an unknown prompt is invalid params', async () => {
  const error = await errorOf(
    '{"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"nope"}}'
  );
  assert.equal(error.code, -32602);
});

// --- lifecycle and errors --------------------------------------------------

test('ping returns an empty result', async () => {
  assert.deepEqual(await resultOf('{"jsonrpc":"2.0","id":9,"method":"ping"}'), {});
});

test('an unknown method is -32601', async () => {
  const error = await errorOf('{"jsonrpc":"2.0","id":1,"method":"server/nonsense"}');
  assert.equal(error.code, -32601);
});

test('invented methods belonging to no MCP revision are not served', async () => {
  for (const method of ['mcp/listTools', 'tools/invoke', 'server/capabilities']) {
    const error = await errorOf(`{"jsonrpc":"2.0","id":1,"method":"${method}"}`);
    assert.equal(error.code, -32601, `${method} must not be served`);
  }
});

test('initialize is rejected by default but names the supported versions', async () => {
  const error = await errorOf('{"jsonrpc":"2.0","id":1,"method":"initialize"}');
  assert.equal(error.code, -32601);
  assert.deepEqual(error.data.supported, ['2026-07-28']);
});

test('dual-era mode answers initialize and advertises both versions', async () => {
  const dual = createMcpDispatcher(fixtureService, { legacyInitialize: true });

  const init = JSON.parse(await dual.handleLine('{"jsonrpc":"2.0","id":1,"method":"initialize"}'));
  assert.equal(init.result.protocolVersion, '2025-11-25');
  assert.equal(init.result.serverInfo.name, 'fixture-server');

  const discover = JSON.parse(
    await dual.handleLine('{"jsonrpc":"2.0","id":2,"method":"server/discover"}')
  );
  assert.deepEqual(discover.result.supportedVersions, ['2026-07-28', '2025-11-25']);
});

test('an unsupported protocol version reports -32022 with the supported list', async () => {
  const error = await errorOf(
    '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"1900-01-01"}}}'
  );
  assert.equal(error.code, -32022);
  assert.equal(error.message, 'Unsupported protocol version');
  assert.deepEqual(error.data.supported, ['2026-07-28']);
  assert.equal(error.data.requested, '1900-01-01');
});

test('a matching protocol version in _meta is accepted', async () => {
  const result = await resultOf(
    '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}'
  );
  assert.ok(Array.isArray(result.tools));
});

// --- notifications ---------------------------------------------------------

test('notifications receive no response', async () => {
  for (const line of [
    '{"jsonrpc":"2.0","method":"notifications/initialized"}',
    '{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}',
    '{"jsonrpc":"2.0","method":"notifications/unknown"}',
  ]) {
    assert.equal(await dispatcher().handleLine(line), null, `must not answer: ${line}`);
  }
});

test('an explicit null id is a request and is answered', async () => {
  const response = await call('{"jsonrpc":"2.0","id":null,"method":"ping"}');
  assert.equal(response.id, null);
  assert.deepEqual(response.result, {});
});

// --- malformed input -------------------------------------------------------

test('invalid JSON is a parse error with a null id', async () => {
  const response = await call('{"jsonrpc":"2.0","id":1,"method":');
  assert.equal(response.error.code, -32700);
  assert.equal(response.id, null);
});

test('a missing method is an invalid request that echoes the id', async () => {
  const response = await call('{"jsonrpc":"2.0","id":7}');
  assert.equal(response.error.code, -32600);
  assert.equal(response.id, 7);
});

test('a wrong jsonrpc version is an invalid request', async () => {
  for (const line of [
    '{"jsonrpc":"1.0","id":1,"method":"ping"}',
    '{"jsonrpc":"2.1","id":1,"method":"ping"}',
  ]) {
    assert.equal((await errorOf(line)).code, -32600, line);
  }
});

test('the TOON-RPC dialect is not accepted as MCP wire', async () => {
  // Spec #389 §9: TOON-RPC extensions must never pass as standard MCP wire.
  const error = await errorOf('{"toonrpc":"1.0","id":1,"method":"tools/list"}');
  assert.equal(error.code, -32600);
});

test('blank and whitespace lines are ignored', async () => {
  for (const line of ['', '   ', '\t']) {
    assert.equal(await dispatcher().handleLine(line), null);
  }
});

test('a non-object message is an invalid request', async () => {
  for (const line of ['[]', '42', '"hello"', 'null']) {
    assert.equal((await errorOf(line)).code, -32600, line);
  }
});

test('params of the wrong type do not throw', async () => {
  const error = await errorOf('{"jsonrpc":"2.0","id":1,"method":"tools/call","params":"nope"}');
  assert.equal(error.code, -32602);
});

test('a string id is echoed unchanged', async () => {
  const response = await call('{"jsonrpc":"2.0","id":"abc-123","method":"ping"}');
  assert.equal(response.id, 'abc-123');
});

test('control characters in arguments stay escaped on one line', async () => {
  const raw = await dispatcher().handleLine(
    '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"text":"a\\nb"}}}'
  );
  assert.ok(!raw.includes('\n'), `an embedded newline would corrupt the framing: ${raw}`);
  assert.equal(JSON.parse(raw).result.content[0].text, 'a\nb');
});
