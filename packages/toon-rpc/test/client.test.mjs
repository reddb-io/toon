import assert from 'node:assert/strict';
import { test } from 'node:test';
import { decode, encode } from '@reddb-io/toon';
import {
  Client,
  ClientAbortError,
  ClientClosedError,
  ClientProtocolError,
  ClientTimeoutError,
  RpcError,
} from '../dist/index.js';

const bytes = (value) => new TextEncoder().encode(encode(value));
const value = (document) => decode(new TextDecoder('utf8', { fatal: true }).decode(document));

test('duplex client correlates typed IDs and owns its receive pump', async () => {
  const transport = new FakeDuplexTransport();
  const diagnostics = [];
  const client = new Client(transport, { onDiagnostic: (entry) => diagnostics.push(entry) });
  const calls = [
    client.call('number', undefined, { id: 1 }),
    client.call('string', {}, { id: '1' }),
    client.call('null', [], { id: null }),
  ];
  await transport.waitForSends(3);

  transport.push(
    bytes([
      { toonrpc: '1.0', result: 'null', id: null },
      { toonrpc: '1.0', result: 'string', id: '1' },
      { toonrpc: '1.0', result: 'number', id: 1 },
    ])
  );
  assert.deepEqual(await Promise.all(calls), ['number', 'string', 'null']);
  assert.equal(client.pendingCallCount, 0);
  assert.deepEqual(diagnostics, []);
  assert.deepEqual(transport.sent.map((document) => value(document).id), [1, '1', null]);
  await client.close();
});

test('RPC errors preserve data presence and notifications never become pending', async () => {
  const transport = new FakeDuplexTransport();
  const client = new Client(transport);
  const absent = client.call('absent', undefined, { id: 2 });
  const present = client.call('present', undefined, { id: 3 });
  await transport.waitForSends(2);
  transport.push(
    bytes([
      { toonrpc: '1.0', error: { code: 1000, message: 'absent' }, id: 2 },
      { toonrpc: '1.0', error: { code: 1001, message: 'present', data: null }, id: 3 },
    ])
  );
  await assert.rejects(absent, (error) => error instanceof RpcError && !error.hasData);
  await assert.rejects(
    present,
    (error) => error instanceof RpcError && error.hasData && error.data === null
  );

  await client.notify('notice');
  assert.equal(client.pendingCallCount, 0);
  assert.equal(Object.hasOwn(value(transport.sent.at(-1)), 'id'), false);
  await client.close();
});

test('batch diagnostics isolate malformed, duplicate, and unknown entries', async () => {
  const transport = new FakeDuplexTransport();
  const diagnostics = [];
  const client = new Client(transport, { onDiagnostic: (entry) => diagnostics.push(entry) });
  const first = client.call('first', undefined, { id: 10 });
  const remaining = observe(client.call('remaining', undefined, { id: 11 }));
  await transport.waitForSends(2);
  transport.push(
    bytes([
      { toonrpc: '1.0', result: 'first', id: 10 },
      { toonrpc: '1.0', result: 'duplicate', id: 10 },
      { toonrpc: '1.0', result: 1, error: { code: -32603, message: 'bad' }, id: 11 },
      { toonrpc: '1.0', result: 'unknown', id: 'missing' },
    ])
  );
  assert.equal(await first, 'first');
  await waitFor(() => diagnostics.length === 3);
  assert.deepEqual(
    diagnostics.map(({ reason, id, index }) => ({ reason, ...(id === undefined ? {} : { id }), index })),
    [
      { reason: 'duplicate-id', id: 10, index: 1 },
      { reason: 'invalid-response', index: 2 },
      { reason: 'unknown-id', id: 'missing', index: 3 },
    ]
  );
  assert.equal(client.pendingCallCount, 1);
  await client.close();
  assert.equal((await remaining).error instanceof ClientClosedError, true);
});

test('invalid documents are diagnosed without silently settling calls', async () => {
  const transport = new FakeDuplexTransport();
  const diagnostics = [];
  const client = new Client(transport, { onDiagnostic: (entry) => diagnostics.push(entry.reason) });
  const pending = observe(client.call('pending', undefined, { id: 20 }));
  await transport.waitForSends(1);
  transport.push(Uint8Array.of(0xff));
  transport.push(bytes([]));
  transport.push(bytes({ toonrpc: '1.0', result: 1, error: { code: 1, message: 'bad' }, id: 20 }));
  await waitFor(() => diagnostics.length === 3);
  assert.deepEqual(diagnostics, ['parse-error', 'invalid-response', 'invalid-response']);
  assert.equal(client.pendingCallCount, 1);
  await client.close();
  assert.equal((await pending).error instanceof ClientClosedError, true);
});

test('abort, timeout, send failure, and invalid input remove pending calls', async () => {
  const transport = new FakeDuplexTransport();
  const client = new Client(transport);
  const controller = new AbortController();
  const aborted = client.call('abort', undefined, { id: 30, signal: controller.signal });
  controller.abort();
  await assert.rejects(aborted, ClientAbortError);

  const timedOut = client.call('timeout', undefined, { id: 31, timeoutMs: 1 });
  await assert.rejects(timedOut, ClientTimeoutError);

  transport.sendError = new Error('send failed');
  await assert.rejects(client.call('send-failure', undefined, { id: 32 }), /send failed/);
  transport.sendError = undefined;
  await assert.rejects(client.call('', undefined, { id: 33 }), /Invalid TOON-RPC request/);
  await assert.rejects(client.call('invalid', 1, { id: 34 }), /Invalid TOON-RPC request/);
  await assert.rejects(client.call('timeout', undefined, { timeoutMs: 2147483648 }), /timeout/);
  assert.equal(client.pendingCallCount, 0);
  await client.close();
});

test('open failure, receive failure, completion, and explicit close reject all calls', async () => {
  const openFailure = new FakeDuplexTransport();
  openFailure.openError = new Error('open failed');
  const failedClient = new Client(openFailure);
  await assert.rejects(failedClient.call('open'), /open failed/);
  assert.equal(failedClient.status, 'failed');

  for (const event of ['failure', 'completion']) {
    const transport = new FakeDuplexTransport();
    const client = new Client(transport);
    const call = client.call(event);
    await transport.waitForSends(1);
    if (event === 'failure') transport.fail(new Error('receive failed'));
    else transport.finish();
    await assert.rejects(call, event === 'failure' ? /receive failed/ : ClientClosedError);
    assert.equal(client.pendingCallCount, 0);
    assert.equal(client.status, event === 'failure' ? 'failed' : 'closed');
    await client.close();
    assert.equal(transport.closeCount, 1);
  }

  const transport = new FakeDuplexTransport();
  const client = new Client(transport);
  const call = client.call('close');
  await transport.waitForSends(1);
  await client.close();
  await assert.rejects(call, ClientClosedError);
  await assert.rejects(client.call('late'), ClientClosedError);
  assert.equal(transport.closeCount, 1);
});

test('notification abort, timeout, and close cover opening and in-flight sends', async () => {
  const timedOpen = deferred();
  const timeoutClient = new Client({
    kind: 'request-response',
    open: () => timedOpen.promise,
    async request() {
      assert.fail('timed-out notification must not send after open');
    },
    async close() {},
  });
  await assert.rejects(timeoutClient.notify('timeout', undefined, { timeoutMs: 1 }), ClientTimeoutError);
  timedOpen.resolve();
  await turn();
  await timeoutClient.close();

  const abortedOpen = deferred();
  const controller = new AbortController();
  const abortClient = new Client({
    kind: 'request-response',
    open: () => abortedOpen.promise,
    async request() {
      assert.fail('aborted notification must not send after open');
    },
    async close() {},
  });
  const aborted = abortClient.notify('abort', undefined, { signal: controller.signal });
  controller.abort();
  await assert.rejects(aborted, ClientAbortError);
  abortedOpen.resolve();
  await turn();
  await abortClient.close();

  const transport = new FakeDuplexTransport();
  transport.sendHook = () => new Promise(() => {});
  const client = new Client(transport);
  const notification = client.notify('close');
  const notificationRejected = assert.rejects(notification, ClientClosedError);
  await transport.waitForSends(1);
  await client.close();
  await notificationRejected;
  assert.equal(transport.lastOperationSignal.aborted, true);
});

test('start and receive cleanup settle even when transport lifecycle methods do not cooperate', async () => {
  let openSignal;
  const hangingOpen = new Client({
    kind: 'request-response',
    open(options) {
      openSignal = options.signal;
      return new Promise(() => {});
    },
    async request() {
      assert.fail('closed client must not request');
    },
    async close() {},
  });
  const starting = observe(hangingOpen.start());
  await turn();
  await hangingOpen.close();
  assert.equal((await starting).error instanceof ClientClosedError, true);
  assert.equal(openSignal.aborted, true);

  let cleaned = false;
  const closeError = new Error('close failed');
  const failingClose = new Client({
    kind: 'duplex',
    async send() {},
    async *receive(options) {
      try {
        await new Promise((resolve) => options.signal.addEventListener('abort', resolve, { once: true }));
      } finally {
        await turn();
        cleaned = true;
      }
    },
    async close() {
      throw closeError;
    },
  });
  await failingClose.start();
  await assert.rejects(failingClose.close(), closeError);
  assert.equal(cleaned, true);
});

test('a response wins exactly once over a later send failure', async () => {
  const transport = new FakeDuplexTransport();
  const send = deferred();
  transport.sendHook = () => send.promise;
  const client = new Client(transport);
  const call = client.call('race', undefined, { id: 40 });
  await transport.waitForSends(1);
  transport.push(bytes({ toonrpc: '1.0', result: 'response', id: 40 }));
  assert.equal(await call, 'response');
  send.reject(new Error('late send failure'));
  await turn();
  assert.equal(client.pendingCallCount, 0);
  await client.close();
});

test('request/response transport owns each direct response', async () => {
  const diagnostics = [];
  const transport = {
    kind: 'request-response',
    async request(document) {
      const request = value(document);
      if (request.method === 'none') return undefined;
      if (request.method === 'wrong') return bytes({ toonrpc: '1.0', result: 1, id: 'other' });
      return Object.hasOwn(request, 'id')
        ? bytes({ toonrpc: '1.0', result: request.params ?? null, id: request.id })
        : undefined;
    },
    async close() {},
  };
  const client = new Client(transport, { onDiagnostic: (entry) => diagnostics.push(entry.reason) });
  assert.deepEqual(await client.call('echo', { ok: true }, { id: null }), { ok: true });
  await client.notify('notice');
  await assert.rejects(client.call('none'), ClientProtocolError);
  await assert.rejects(client.call('wrong'), ClientProtocolError);
  assert.deepEqual(diagnostics, ['unknown-id']);
  assert.equal(client.pendingCallCount, 0);
  await client.close();
});

test('direct exchanges cannot settle another call or steal from a notification', async () => {
  const secondResponse = deferred();
  const heldResponse = deferred();
  const diagnostics = [];
  const transport = {
    kind: 'request-response',
    async request(document) {
      const request = value(document);
      if (request.method === 'first') {
        return bytes({ toonrpc: '1.0', result: 'stolen', id: 52 });
      }
      if (request.method === 'second') return secondResponse.promise;
      if (request.method === 'held') return heldResponse.promise;
      return bytes({ toonrpc: '1.0', result: 'notification theft', id: 53 });
    },
    async close() {},
  };
  const client = new Client(transport, { onDiagnostic: (entry) => diagnostics.push(entry) });
  const first = client.call('first', undefined, { id: 51 });
  const second = observe(client.call('second', undefined, { id: 52 }));
  await assert.rejects(first, ClientProtocolError);
  assert.equal(client.pendingCallCount, 1);
  secondResponse.resolve(bytes({ toonrpc: '1.0', result: 'owned', id: 52 }));
  assert.deepEqual(await second, { result: 'owned' });

  const held = observe(client.call('held', undefined, { id: 53 }));
  await client.notify('notice');
  assert.equal(client.pendingCallCount, 1);
  heldResponse.resolve(bytes({ toonrpc: '1.0', result: 'held', id: 53 }));
  assert.deepEqual(await held, { result: 'held' });
  assert.deepEqual(
    diagnostics.map(({ reason, id }) => ({ reason, id })),
    [
      { reason: 'unknown-id', id: 52 },
      { reason: 'unknown-id', id: 53 },
    ]
  );
  await client.close();
});

class FakeDuplexTransport {
  kind = 'duplex';
  sent = [];
  sendError;
  sendHook;
  openError;
  closeCount = 0;
  lastOperationSignal;
  #events = [];
  #waiters = [];

  async open() {
    if (this.openError) throw this.openError;
  }

  async send(document, options) {
    this.sent.push(document);
    this.lastOperationSignal = options?.signal;
    if (this.sendError) throw this.sendError;
    await this.sendHook?.(document);
  }

  async *receive(options) {
    this.receiveSignal = options?.signal;
    while (true) {
      const event = this.#events.shift() ?? (await new Promise((resolve) => this.#waiters.push(resolve)));
      if (event.kind === 'document') yield event.document;
      else if (event.kind === 'error') throw event.error;
      else return;
    }
  }

  push(document) {
    this.#emit({ kind: 'document', document });
  }

  fail(error) {
    this.#emit({ kind: 'error', error });
  }

  finish() {
    this.#emit({ kind: 'finish' });
  }

  async close() {
    this.closeCount += 1;
    this.finish();
  }

  async waitForSends(count) {
    await waitFor(() => this.sent.length >= count);
  }

  #emit(event) {
    const waiter = this.#waiters.shift();
    if (waiter) waiter(event);
    else this.#events.push(event);
  }
}

function observe(promise) {
  return promise.then(
    (result) => ({ result }),
    (error) => ({ error })
  );
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function waitFor(predicate) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (predicate()) return;
    await turn();
  }
  assert.fail('condition did not become true');
}

const turn = () => new Promise((resolve) => setImmediate(resolve));
