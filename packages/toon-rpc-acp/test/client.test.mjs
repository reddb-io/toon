import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { after, test } from 'node:test';
import { decode, encode } from '@reddb-io/toon';
import { callAgent, listAgents } from '../dist/index.js';

const PARTS = [{ kind: 'text', content_type: 'text/plain', content: 'hello', status: 'done' }];

const RUN = {
  agentRunId: '5f0b2f5e-0000-4000-8000-000000000000',
  agentName: 'echo',
  status: 'completed',
  input: { parts: PARTS },
  output: [{ role: 'assistant', parts: PARTS }],
};

const AGENTS = [{ name: 'echo', description: 'Echoes back.', version: '0.1.0' }];

/**
 * A stand-in for the legacy ACP server: it records what the client sent and
 * answers in whichever encoding the request asked for.
 */
async function startServer(handler) {
  const seen = [];
  const server = createServer((req, res) => {
    const chunks = [];
    req.on('data', (chunk) => chunks.push(chunk));
    req.on('end', () => {
      const request = {
        method: req.method,
        url: req.url,
        accept: req.headers.accept,
        contentType: req.headers['content-type'],
        body: Buffer.concat(chunks).toString('utf8'),
      };
      seen.push(request);
      handler(request, res);
    });
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const baseUrl = `http://127.0.0.1:${server.address().port}`;
  return {
    baseUrl,
    seen,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

function respond(res, status, accept, value) {
  const toon = (accept ?? '').includes('application/toon');
  res.writeHead(status, {
    'Content-Type': toon ? 'application/toon' : 'application/json',
  });
  res.end(toon ? encode(value) : JSON.stringify(value));
}

const openServers = [];
async function serve(handler) {
  const server = await startServer(handler);
  openServers.push(server);
  return server;
}
after(async () => {
  await Promise.all(openServers.map((server) => server.close()));
});

test('callAgent defaults to JSON on both the request body and the response', async () => {
  const server = await serve((request, res) => respond(res, 200, request.accept, RUN));

  const run = await callAgent(server.baseUrl, 'echo', PARTS);

  assert.deepEqual(run, RUN);
  assert.equal(server.seen.length, 1);
  assert.equal(server.seen[0].method, 'POST');
  assert.equal(server.seen[0].url, '/agents/echo/runs');
  assert.equal(server.seen[0].accept, 'application/json');
  assert.equal(server.seen[0].contentType, 'application/json');
  assert.deepEqual(JSON.parse(server.seen[0].body), { parts: PARTS });
});

test('the toon option switches the request body and the response parsing together', async () => {
  const server = await serve((request, res) => respond(res, 200, request.accept, RUN));

  const run = await callAgent(server.baseUrl, 'echo', PARTS, { toon: true });

  assert.deepEqual(run, RUN);
  assert.equal(server.seen[0].accept, 'application/toon');
  assert.equal(server.seen[0].contentType, 'application/toon');
  assert.notEqual(server.seen[0].body, JSON.stringify({ parts: PARTS }));
  assert.equal(server.seen[0].body, encode({ parts: PARTS }));
  assert.deepEqual(decode(server.seen[0].body), { parts: PARTS });
});

test('listAgents round trips in both encodings', async () => {
  const server = await serve((request, res) => respond(res, 200, request.accept, AGENTS));

  assert.deepEqual(await listAgents(server.baseUrl), AGENTS);
  assert.equal(server.seen[0].accept, 'application/json');

  assert.deepEqual(await listAgents(server.baseUrl, { toon: true }), AGENTS);
  assert.equal(server.seen[1].accept, 'application/toon');
});

test('HTTP error statuses are reported, not parsed as a run', async () => {
  const server = await serve((request, res) => {
    respond(res, 404, request.accept, { error: 'agent not found: nope' });
  });

  await assert.rejects(callAgent(server.baseUrl, 'nope', PARTS), /ACP call failed: 404/);
  await assert.rejects(listAgents(server.baseUrl), /ACP list failed: 404/);
});

test('a malformed JSON body fails with a decoding error, not a silent value', async () => {
  const server = await serve((_request, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end('{not json');
  });

  await assert.rejects(callAgent(server.baseUrl, 'echo', PARTS), /invalid JSON/);
  await assert.rejects(listAgents(server.baseUrl), /invalid JSON/);
});

test('timeoutMs aborts a hanging request', async () => {
  const server = await serve(() => {
    /* never responds */
  });

  await assert.rejects(
    callAgent(server.baseUrl, 'echo', PARTS, { timeoutMs: 50 }),
    (error) => error instanceof Error,
  );
});

test('an already-aborted signal aborts the request', async () => {
  const server = await serve((request, res) => respond(res, 200, request.accept, RUN));
  const controller = new AbortController();
  controller.abort();

  await assert.rejects(
    callAgent(server.baseUrl, 'echo', PARTS, { signal: controller.signal }),
    (error) => error instanceof Error,
  );
});

test('a caller signal aborts a hanging request even when a timeout is set', async () => {
  const server = await serve(() => {
    /* never responds */
  });
  const controller = new AbortController();
  setTimeout(() => controller.abort(new Error('caller gave up')), 25);

  await assert.rejects(
    callAgent(server.baseUrl, 'echo', PARTS, { signal: controller.signal, timeoutMs: 10_000 }),
    /caller gave up/,
  );
});
