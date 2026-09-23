import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { decode } from '@reddb-io/toon';
import { RpcError } from '@reddb-io/toon-rpc';
import { MultiRpc, Server } from '../dist/index.js';

const corpus = JSON.parse(
  readFileSync(new URL('../../../tests/corpus/toon-rpc/multi.json', import.meta.url), 'utf8')
);
const CASE_COUNT = 19;

function multi() {
  const server = new Server();
  server.register('echo', async (params) => (Array.isArray(params) ? params[0] : null));
  server.register('fail', async () => {
    throw new RpcError(1, 'failed');
  });
  return new MultiRpc(server);
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

test('the shared mixed-dialect corpus', async () => {
  assert.equal(corpus.schemaVersion, 'toon-rpc-multi-v1');
  assert.equal(corpus.cases.length, CASE_COUNT);
  for (const fixture of corpus.cases) {
    const { protocol, body } = await multi().handleWithProtocol(fixture.raw, fixture.contentType);
    assert.equal(protocol, fixture.expect.protocol, `${fixture.name}: protocol`);
    if (fixture.expect.response === null) {
      assert.equal(body.length, 0, `${fixture.name}: no response`);
      continue;
    }
    const text = new TextDecoder().decode(body);
    const response = protocol === 'jsonrpc' ? JSON.parse(text) : decode(text);
    matches(response, fixture.expect.response, fixture.name);
  }
});
