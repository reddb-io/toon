import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { isDeepStrictEqual } from 'node:util';
import { decode, encode } from '@reddb-io/toon';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import { RpcError, Server, snapshotResponse } from '../dist/index.js';

const SCHEMA_VERSION = 'toon-rpc-fixtures-v1';
const PROTOCOL_VERSION = '1.0';
const CHECKPOINT = {
  version: '4.1.1',
  repository: 'toon-format/spec',
  revision: '62f16b369408180f1faf1cba7da1b46d1f336f12',
};
const corpusUrl = new URL('../../../tests/corpus/toon-rpc/contract.json', import.meta.url);
const schemaUrl = new URL('../../../tests/corpus/toon-rpc/fixtures.schema.json', import.meta.url);
const hasOwn = (value, key) => Object.prototype.hasOwnProperty.call(value, key);

const [corpus, schema] = await Promise.all(
  [corpusUrl, schemaUrl].map(async (url) => JSON.parse(await readFile(url, 'utf8')))
);
const validateCorpus = compileCorpusSchema(schema);
validateContractSchema(corpus);
const cases = [...corpus.valid, ...corpus.malformed];

preflight();

test('shared TOON-RPC contract corpus (66 cases)', async (t) => {
  let serverCount = 0;
  let clientCount = 0;
  for (const fixture of cases) {
    await t.test(fixture.name, async () => {
      const raw = materialize(fixture);
      if (fixture.direction === 'server') {
        serverCount += 1;
        await runServerCase(fixture, raw);
      } else {
        clientCount += 1;
        runClientCase(fixture, raw);
      }
    });
  }
  assert.equal(serverCount, 43);
  assert.equal(clientCount, 23);
});

test('Draft 2020-12 schema rejects contract mutations', () => {
  const mutations = [
    ['unknown field', (value) => (value.unknown = true)],
    ['schemaVersion', (value) => (value.schemaVersion = 'wrong')],
    ['name pattern', (value) => (contractCase(value, 'request/positional-params').name = 'Invalid Name')],
    [
      'data presence',
      (value) => (contractCase(value, 'error/application-code-without-data').expect.data = null),
    ],
    [
      'ordered batch',
      (value) => (contractCase(value, 'batch/mixed-request-and-notification').expect.ordered = true),
    ],
  ];
  for (const [name, mutate] of mutations) {
    const changed = structuredClone(corpus);
    mutate(changed);
    assert.throws(
      () => validateContractSchema(changed),
      /contract schema validation failed:/,
      `${name} mutation`
    );
  }
});

test('response matcher validates Error Objects and scopes unknown members', () => {
  const expected = contractCase(corpus, 'error/application-code-without-data').expect;
  const response = {
    toonrpc: '1.0',
    error: { code: 1000, message: 'fixture failure', extension: true },
    id: 6,
  };
  assert.equal(responseMatches(response, expected, false), true);
  assert.equal(responseMatches(response, expected, true), false);
  delete response.error.message;
  assert.equal(responseMatches(response, expected, false), false);
});

function compileCorpusSchema(document) {
  try {
    const ajv = new Ajv2020({ allErrors: true, strict: true });
    addFormats(ajv);
    return ajv.compile(document);
  } catch (error) {
    throw new Error(`contract schema compile failed: ${error instanceof Error ? error.message : error}`);
  }
}

function validateContractSchema(value) {
  if (validateCorpus(value)) return;
  const summary = validateCorpus.errors
    .map((error) => `${error.instancePath || '/'} ${error.message} (${error.schemaPath})`)
    .join('\n');
  throw new Error(`contract schema validation failed:\n${summary}`);
}

function contractCase(document, name) {
  const fixture = [...document.valid, ...document.malformed].find((entry) => entry.name === name);
  assert.ok(fixture, `missing contract case ${name}`);
  return fixture;
}

function preflight() {
  assert.equal(schema.$schema, 'https://json-schema.org/draft/2020-12/schema');
  assert.equal(schema.$id, 'https://reddb.io/schemas/toon-rpc-fixtures-v1.json');
  assert.equal(schema.$defs.input.type, 'object');
  assert.equal(schema.$defs.case.type, 'object');
  assert.equal(schema.properties.schemaVersion.const, SCHEMA_VERSION);
  assert.equal(schema.properties.protocolVersion.const, PROTOCOL_VERSION);
  assert.deepEqual(
    {
      version: schema.$defs.toonCheckpoint.properties.version.const,
      repository: schema.$defs.toonCheckpoint.properties.repository.const,
      revision: schema.$defs.toonCheckpoint.properties.revision.const,
    },
    CHECKPOINT
  );

  assert.equal(corpus.$schema, './fixtures.schema.json');
  assert.equal(corpus.schemaVersion, SCHEMA_VERSION);
  assert.equal(corpus.protocolVersion, PROTOCOL_VERSION);
  assert.deepEqual(corpus.toonCheckpoint, CHECKPOINT);
  validateHandlers();
  assert.equal(cases.length, 66);
  assert.equal(cases.filter(({ direction }) => direction === 'server').length, 43);
  assert.equal(cases.filter(({ direction }) => direction === 'client').length, 23);

  const names = new Set();
  for (const fixture of cases) {
    assert.equal(names.has(fixture.name), false, `duplicate case name: ${fixture.name}`);
    names.add(fixture.name);
    assert.equal(fixture.encoding, 'toon', `${fixture.name}: encoding`);
    assert.match(fixture.direction, /^(server|client)$/, `${fixture.name}: direction`);

    const sourceForm = Object.keys(fixture.input)
      .filter((key) => key !== 'pendingIds')
      .sort()
      .join('+');
    assert.equal(
      ['bytesBase64', 'value', 'value+wire', 'wire'].includes(sourceForm),
      true,
      `${fixture.name}: source form`
    );

    if (hasOwn(fixture.input, 'wire')) {
      assert.equal(typeof fixture.input.wire, 'string', `${fixture.name}: wire type`);
    }
    if (hasOwn(fixture.input, 'value')) {
      validateCoreFixtureValue(fixture.input.value, fixture.name);
    }
    if (hasOwn(fixture.input, 'bytesBase64')) {
      assert.equal(typeof fixture.input.bytesBase64, 'string', `${fixture.name}: base64 type`);
    }

    if (hasOwn(fixture.input, 'wire') && hasOwn(fixture.input, 'value')) {
      assert.deepEqual(
        decode(fixture.input.wire),
        fixture.input.value,
        `${fixture.name}: paired wire/value`
      );
    }
    if (hasOwn(fixture.input, 'bytesBase64')) {
      const encoded = fixture.input.bytesBase64;
      assert.equal(encoded.length >= 4, true, `${fixture.name}: base64 length`);
      assert.match(encoded, /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/);
      assert.equal(Buffer.from(encoded, 'base64').toString('base64'), encoded, `${fixture.name}: base64`);
    }

    if (fixture.direction === 'server') {
      assert.equal(
        Number.isSafeInteger(fixture.expect.callCount) && fixture.expect.callCount >= 0,
        true,
        `${fixture.name}: callCount`
      );
    } else {
      assert.equal(hasOwn(fixture.expect, 'callCount'), false, `${fixture.name}: client callCount`);
      assert.equal(hasOwn(fixture.expect, 'calls'), false, `${fixture.name}: client calls`);
    }
    if (hasOwn(fixture.expect, 'calls')) {
      assert.equal(
        fixture.expect.calls !== null &&
          typeof fixture.expect.calls === 'object' &&
          !Array.isArray(fixture.expect.calls),
        true,
        `${fixture.name}: calls map`
      );
      assert.equal(
        Object.keys(fixture.expect.calls).every((method) => hasOwn(corpus.handlers, method)),
        true,
        `${fixture.name}: calls methods`
      );
      const sum = Object.values(fixture.expect.calls).reduce((total, count) => total + count, 0);
      assert.equal(sum, fixture.expect.callCount, `${fixture.name}: calls sum`);
      assert.equal(
        Object.values(fixture.expect.calls).every((count) => Number.isInteger(count) && count > 0),
        true,
        `${fixture.name}: calls values`
      );
    }

    if (fixture.direction === 'client') {
      assert.equal(hasOwn(fixture.input, 'pendingIds'), true, `${fixture.name}: pendingIds`);
      assert.equal(
        fixture.input.pendingIds.every(
          (id) => id === null || typeof id === 'string' || Number.isSafeInteger(id)
        ),
        true,
        `${fixture.name}: pending ID type`
      );
      // Map uses SameValueZero with type-sensitive keys: 1, "1", and null stay distinct.
      assert.equal(new Map(fixture.input.pendingIds.map((id) => [id, true])).size, fixture.input.pendingIds.length);
    } else {
      assert.equal(hasOwn(fixture.input, 'pendingIds'), false, `${fixture.name}: server pendingIds`);
    }
  }
}

function validateHandlers() {
  assert.equal(
    corpus.handlers !== null && typeof corpus.handlers === 'object' && !Array.isArray(corpus.handlers),
    true,
    'handlers must be an object'
  );
  assert.notEqual(Object.keys(corpus.handlers).length, 0, 'handlers must not be empty');
  for (const [method, definition] of Object.entries(corpus.handlers)) {
    assert.notEqual(method.length, 0, 'handler method must not be empty');
    assert.equal(
      definition !== null && typeof definition === 'object' && !Array.isArray(definition),
      true,
      `handler ${method}: definition`
    );
    let members;
    switch (definition.kind) {
      case 'result':
        assert.equal(hasOwn(definition, 'value'), true, `handler ${method}: value`);
        validateCoreFixtureValue(definition.value, method);
        members = ['kind', 'value'];
        break;
      case 'error':
        assert.equal(
          Number.isInteger(definition.code) && definition.code >= -2147483648 && definition.code <= 2147483647,
          true,
          `handler ${method}: code`
        );
        assert.equal(typeof definition.message, 'string', `handler ${method}: message`);
        if (hasOwn(definition, 'data')) validateCoreFixtureValue(definition.data, method);
        members = hasOwn(definition, 'data')
          ? ['code', 'data', 'kind', 'message']
          : ['code', 'kind', 'message'];
        break;
      case 'echo-params':
      case 'internal-error':
        members = ['kind'];
        break;
      case 'reject-params':
        assert.equal(typeof definition.message, 'string', `handler ${method}: message`);
        members = ['kind', 'message'];
        break;
      default:
        assert.fail(`handler ${method}: unknown kind ${definition.kind}`);
    }
    assert.deepEqual(Object.keys(definition).sort(), members, `handler ${method}: exact members`);
  }
}

function validateCoreFixtureValue(root, context) {
  const pending = [root];
  while (pending.length > 0) {
    const value = pending.pop();
    if (value === null || typeof value === 'boolean' || typeof value === 'string') continue;
    if (typeof value === 'number') {
      assert.equal(
        Number.isFinite(value) && (!Number.isInteger(value) || Number.isSafeInteger(value)),
        true,
        `${context}: core fixture number`
      );
      continue;
    }
    assert.equal(typeof value, 'object', `${context}: core fixture type`);
    pending.push(...Object.values(value));
  }
}

function materialize(fixture) {
  if (hasOwn(fixture.input, 'wire')) return new TextEncoder().encode(fixture.input.wire);
  if (hasOwn(fixture.input, 'bytesBase64')) return Uint8Array.from(Buffer.from(fixture.input.bytesBase64, 'base64'));
  return new TextEncoder().encode(encode(fixture.input.value));
}

async function runServerCase(fixture, raw) {
  const server = new Server();
  const calls = new Map();
  for (const [method, definition] of Object.entries(corpus.handlers)) {
    server.register(method, async (params) => {
      calls.set(method, (calls.get(method) ?? 0) + 1);
      switch (definition.kind) {
        case 'result':
          return structuredClone(definition.value);
        case 'echo-params':
          return params;
        case 'error':
          throw hasOwn(definition, 'data')
            ? new RpcError(definition.code, definition.message, structuredClone(definition.data))
            : new RpcError(definition.code, definition.message);
        case 'reject-params':
          throw new RpcError(-32602, definition.message);
        case 'internal-error':
          throw new Error('fixture internal error');
        default:
          assert.fail(`unknown fixture handler kind: ${definition.kind}`);
      }
    });
  }

  const responseBytes = await server.handle(raw);
  checkCalls(fixture, calls);
  if (fixture.expect.kind === 'no-response') {
    assert.equal(responseBytes.length, 0, `${fixture.name}: no response`);
    return;
  }

  assert.notEqual(responseBytes.length, 0, `${fixture.name}: response missing`);
  const response = decode(new TextDecoder('utf8', { fatal: true }).decode(responseBytes));
  if (fixture.expect.kind === 'batch') {
    assert.equal(fixture.expect.ordered, false, `${fixture.name}: unordered batch contract`);
    assert.equal(Array.isArray(response), true, `${fixture.name}: batch shape`);
    assertUnorderedResponses(response, fixture.expect.responses, fixture.name);
  } else {
    assert.equal(responseMatches(response, fixture.expect, true), true, `${fixture.name}: response`);
  }
}

function checkCalls(fixture, calls) {
  const total = [...calls.values()].reduce((sum, count) => sum + count, 0);
  assert.equal(total, fixture.expect.callCount, `${fixture.name}: callCount`);
  if (hasOwn(fixture.expect, 'calls')) {
    assert.deepEqual(Object.fromEntries(calls), fixture.expect.calls, `${fixture.name}: exact calls`);
  }
}

function runClientCase(fixture, raw) {
  const outcome = clientOracle(raw, fixture.input.pendingIds);
  switch (fixture.expect.kind) {
    case 'accept':
      assert.equal(outcome.kind, 'accept', `${fixture.name}: ${outcome.reason ?? outcome.kind}`);
      break;
    case 'reject':
      assert.deepEqual(outcome, { kind: 'reject', reason: fixture.expect.reason }, fixture.name);
      break;
    case 'client-batch':
      assert.equal(outcome.kind, 'client-batch', `${fixture.name}: ${outcome.reason ?? outcome.kind}`);
      assert.equal(outcome.settled.length, fixture.expect.settled.length, `${fixture.name}: settled count`);
      outcome.settled.forEach((response, index) => {
        assert.equal(
          responseMatches(response, fixture.expect.settled[index], false),
          true,
          `${fixture.name}: settled ${index}`
        );
      });
      assert.deepEqual(outcome.rejected, fixture.expect.rejected, `${fixture.name}: rejected entries`);
      assert.deepEqual(outcome.remainingPendingIds, fixture.expect.remainingPendingIds, `${fixture.name}: remaining`);
      break;
    default:
      assert.fail(`${fixture.name}: unknown client expectation ${fixture.expect.kind}`);
  }
}

// Harness-only oracle: production clients do not yet expose independent batch
// diagnostics. TOON performs wire decoding and snapshotResponse is the protocol
// validator; this code must not be exported as a second client implementation.
function clientOracle(raw, pendingIds) {
  let value;
  try {
    value = decode(new TextDecoder('utf8', { fatal: true }).decode(raw));
  } catch {
    return { kind: 'reject', reason: 'parse-error' };
  }

  const pending = new Map(pendingIds.map((id) => [id, true]));
  if (!Array.isArray(value)) {
    const response = snapshotResponse(value);
    if (!response) return { kind: 'reject', reason: 'invalid-response' };
    if (!pending.has(response.id)) return { kind: 'reject', reason: 'unknown-id' };
    return { kind: 'accept' };
  }
  if (value.length === 0) return { kind: 'reject', reason: 'invalid-response' };

  const settledIds = new Set();
  const settled = [];
  const rejected = [];
  value.forEach((entry, index) => {
    const response = snapshotResponse(entry);
    if (!response) {
      rejected.push({ index, reason: 'invalid-response' });
    } else if (settledIds.has(response.id)) {
      rejected.push({ index, reason: 'duplicate-id' });
    } else if (!pending.has(response.id)) {
      rejected.push({ index, reason: 'unknown-id' });
    } else {
      pending.delete(response.id);
      settledIds.add(response.id);
      settled.push(response);
    }
  });

  return {
    kind: 'client-batch',
    settled,
    rejected,
    remainingPendingIds: pendingIds.filter((id) => pending.has(id)),
  };
}

function responseMatches(actual, expected, generated) {
  if (actual === null || typeof actual !== 'object' || Array.isArray(actual)) return false;
  if (!isDeepStrictEqual(Object.keys(actual).sort(), [...expected.exactMembers].sort())) return false;
  if (actual.toonrpc !== PROTOCOL_VERSION || !isDeepStrictEqual(actual.id, expected.id)) return false;
  if (expected.kind === 'success') return isDeepStrictEqual(actual.result, expected.result);
  if (
    expected.kind !== 'error' ||
    actual.error === null ||
    typeof actual.error !== 'object' ||
    Array.isArray(actual.error)
  ) {
    return false;
  }
  if (
    !hasOwn(actual.error, 'code') ||
    !Number.isInteger(actual.error.code) ||
    actual.error.code < -2147483648 ||
    actual.error.code > 2147483647 ||
    !hasOwn(actual.error, 'message') ||
    typeof actual.error.message !== 'string' ||
    actual.error.code !== expected.code ||
    hasOwn(actual.error, 'data') !== expected.hasData
  ) {
    return false;
  }
  if (expected.hasData && !isDeepStrictEqual(actual.error.data, expected.data)) return false;
  if (generated) {
    const members = expected.hasData ? ['code', 'data', 'message'] : ['code', 'message'];
    if (!isDeepStrictEqual(Object.keys(actual.error).sort(), members)) return false;
  }
  return !hasOwn(expected, 'message') || actual.error.message === expected.message;
}

function assertUnorderedResponses(actual, expected, name) {
  assert.equal(actual.length, expected.length, `${name}: batch count`);
  const unused = [...actual];
  for (const matcher of expected) {
    const index = unused.findIndex((response) => responseMatches(response, matcher, true));
    assert.notEqual(index, -1, `${name}: unmatched response ${JSON.stringify(matcher)}`);
    unused.splice(index, 1);
  }
  assert.equal(unused.length, 0, `${name}: extra responses`);
}
