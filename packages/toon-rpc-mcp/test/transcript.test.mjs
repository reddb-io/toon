import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PassThrough } from 'node:stream';
import { test } from 'node:test';
import { decode } from '@reddb-io/toon';
import { CallToolResult, McpError, McpServer, textContent, toonContent } from '../dist/index.js';
import { serveStdio } from '../dist/stdio.js';

const fixture = JSON.parse(
  readFileSync(new URL('../../../tests/corpus/mcp/transcript.json', import.meta.url), 'utf8')
);
const STEP_COUNT = 26;

/** The service every session of the shared transcript runs against. */
function fixtureService() {
  return {
    serverInfo: { name: fixture.server.name, version: fixture.server.version },
    instructions: fixture.server.instructions,
    tools: {
      list: () => fixture.tools,
      call(name, args) {
        if (name === 'echo') return CallToolResult.text(String(args.text));
        if (name === 'fail') return CallToolResult.error('boom');
        throw McpError.unknownTool(name);
      },
    },
    resources: {
      list: () => fixture.resources,
      read(uri) {
        if (uri !== 'memo://hello') throw McpError.resourceNotFound(uri);
        return { contents: [{ uri, mimeType: 'text/plain', text: 'Hello' }] };
      },
    },
    prompts: {
      list: () => fixture.prompts,
      get(name, args) {
        if (name !== 'greet') throw McpError.invalidParams(`Unknown prompt: ${name}`);
        if (typeof args.name !== 'string') throw McpError.invalidParams('Missing argument: name');
        return {
          description: 'Greet someone.',
          messages: [{ role: 'user', content: textContent(`Hello, ${args.name}!`) }],
        };
      },
    },
  };
}

/** Exact match, except an error message is only compared when expected. */
function matches(actual, expected, path = '') {
  if (expected === null || typeof expected !== 'object') {
    assert.deepEqual(actual, expected, path);
    return;
  }
  if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), `${path}: expected an array`);
    assert.equal(actual.length, expected.length, `${path}: length`);
    expected.forEach((entry, index) => matches(actual[index], entry, `${path}[${index}]`));
    return;
  }
  const keys = Object.keys(actual).filter(
    (key) => !(path.endsWith('.error') && key === 'message' && !('message' in expected))
  );
  assert.deepEqual(keys.sort(), Object.keys(expected).sort(), `${path}: members`);
  for (const key of Object.keys(expected)) matches(actual[key], expected[key], `${path}.${key}`);
}

test('the shared MCP transcript', async () => {
  assert.equal(fixture.schemaVersion, 'mcp-transcript-v1');
  assert.equal(fixture.protocolVersion, '2025-06-18');
  let steps = 0;
  for (const session of fixture.sessions) {
    const server = new McpServer(fixtureService());
    for (const [index, step] of session.steps.entries()) {
      steps += 1;
      const line = 'sendRaw' in step ? step.sendRaw : JSON.stringify(step.send);
      const answer = await server.handleLine(line);
      const where = `${session.name} step ${index}`;
      if (step.expect === null) {
        assert.equal(answer, undefined, `${where}: no response`);
      } else {
        assert.ok(!answer.includes('\n'), `${where}: one line`);
        matches(JSON.parse(answer), step.expect, where);
      }
    }
  }
  assert.equal(steps, STEP_COUNT);
});

test('stdio carries one JSON message per line', async () => {
  const input = new PassThrough();
  const output = new PassThrough();
  const lines = [];
  output.setEncoding('utf8');
  output.on('data', (chunk) => lines.push(...chunk.split('\n').filter(Boolean)));
  const served = serveStdio(fixtureService(), { input, output });
  const [first] = fixture.sessions;
  for (const step of first.steps) input.write(`${JSON.stringify(step.send)}\n`);
  input.end();
  await served;
  const expected = first.steps.filter((step) => step.expect !== null);
  assert.equal(lines.length, expected.length);
  const byId = new Map(lines.map((line) => JSON.parse(line)).map((message) => [message.id, message]));
  for (const step of expected) matches(byId.get(step.expect.id), step.expect, `id ${step.expect.id}`);
});

test('toonContent renders a value as TOON text', () => {
  const result = CallToolResult.toon({ rows: [{ a: 1, b: 'x' }] });
  assert.equal(result.content[0].type, 'text');
  assert.deepEqual(decode(result.content[0].text), { rows: [{ a: 1, b: 'x' }] });
  assert.deepEqual(result.structuredContent, { rows: [{ a: 1, b: 'x' }] });
  assert.deepEqual(toonContent([1, 2]), { type: 'text', text: '[2]: 1,2' });
});
