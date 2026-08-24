import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import {
  isCoreValue,
  isErrorObject,
  isId,
  isNotification,
  isParams,
  isRequestObject,
  isResponse,
  snapshotCoreValue,
  snapshotRequestObject,
} from '../dist/index.js';

test('core values enforce finite numbers, recursively safe integers and supported host shapes', () => {
  assert.equal(isCoreValue({ nested: [null, true, 'text', 1.5, Number.MAX_SAFE_INTEGER] }), true);
  assert.equal(isCoreValue({ nested: [Number.MAX_SAFE_INTEGER + 1] }), false);
  assert.equal(isCoreValue({ nested: Number.NaN }), false);
  assert.equal(isCoreValue(Number.POSITIVE_INFINITY), false);
  assert.equal(isCoreValue({ missing: undefined }), false);
  assert.equal(isCoreValue(1n), false);
  assert.equal(isCoreValue(new Date()), false);
  assert.equal(isCoreValue([, 1]), false);
  assert.equal(isCoreValue('\ud800'), false);

  const cyclic = {};
  cyclic.self = cyclic;
  assert.equal(isCoreValue(cyclic), false);

  const symbolMember = { ok: true };
  symbolMember[Symbol('hidden')] = 1;
  assert.equal(isCoreValue(symbolMember), false);

  const accessor = {};
  Object.defineProperty(accessor, 'value', { enumerable: true, get: () => 1 });
  assert.equal(isCoreValue(accessor), false);

  const descriptorFailure = new Proxy({ value: 1 }, {
    getOwnPropertyDescriptor() {
      throw new Error('descriptor failed');
    },
  });
  assert.equal(snapshotCoreValue(descriptorFailure), undefined);
});

test('snapshot traversal is iterative and rejects deep cycles without using the call stack', () => {
  let deep = null;
  for (let index = 0; index < 20_000; index += 1) deep = { next: deep };
  const snapshot = snapshotCoreValue(deep);
  assert.notEqual(snapshot, undefined);

  let cursor = snapshot;
  for (let index = 0; index < 20_000; index += 1) cursor = cursor.next;
  assert.equal(cursor, null);

  const cyclic = { value: 1 };
  cyclic.next = cyclic;
  assert.equal(snapshotCoreValue(cyclic), undefined);
});

test('snapshot materializes small acyclic aliases as independent local copies', () => {
  const shared = runInNewContext('({ value: 1 })');
  const snapshot = snapshotCoreValue({ left: shared, right: shared });

  assert.deepEqual(snapshot, { left: { value: 1 }, right: { value: 1 } });
  assert.notEqual(snapshot.left, snapshot.right);
  assert.equal(Object.getPrototypeOf(snapshot.left), Object.prototype);
  assert.equal(Object.getPrototypeOf(snapshot.right), Object.prototype);

  snapshot.left.value = 2;
  assert.equal(snapshot.right.value, 1);
  assert.equal(shared.value, 1);
});

test('snapshot bounds exponentially expanding DAG aliases by expansion count', () => {
  let inspections = 0;
  const wrap = (value) =>
    new Proxy(value, {
      ownKeys(target) {
        inspections += 1;
        return Reflect.ownKeys(target);
      },
    });
  let dag = wrap({ value: true });
  for (let depth = 0; depth < 30; depth += 1) dag = wrap({ left: dag, right: dag });

  const started = performance.now();
  assert.equal(snapshotCoreValue(dag), undefined);
  const elapsed = performance.now() - started;

  assert.ok(inspections > 10_000 && inspections <= 25_000);
  assert.ok(elapsed < 5_000, `bounded DAG snapshot took ${Math.round(elapsed)}ms`);
});

test('snapshot node budget bounds Proxies that fabricate an infinite descendant chain', () => {
  let descriptors = 0;
  const expanding = () =>
    new Proxy(
      {},
      {
        ownKeys: () => ['next'],
        getOwnPropertyDescriptor() {
          descriptors += 1;
          return {
            value: expanding(),
            enumerable: true,
            configurable: true,
            writable: true,
          };
        },
      }
    );

  assert.equal(snapshotCoreValue(expanding()), undefined);
  assert.ok(descriptors > 20_000 && descriptors <= 25_000);
});

test('snapshot reserves budget while inspecting a Proxy with 50k own keys', () => {
  const keys = Array.from({ length: 50_000 }, (_, index) => `key${index}`);
  let ownKeyCalls = 0;
  let descriptorCalls = 0;
  const wide = new Proxy(
    {},
    {
      ownKeys() {
        ownKeyCalls += 1;
        return keys;
      },
      getOwnPropertyDescriptor() {
        descriptorCalls += 1;
        return { value: true, enumerable: true, configurable: true, writable: true };
      },
    }
  );

  assert.equal(snapshotCoreValue(wide), undefined);
  assert.equal(ownKeyCalls, 1);
  assert.ok(descriptorCalls > 20_000 && descriptorCalls <= 25_004);
});

test('plain objects and arrays from another realm become local snapshots', () => {
  const foreign = runInNewContext('({ object: { value: 1 }, array: [1, 2] })');
  const snapshot = snapshotCoreValue(foreign);
  assert.deepEqual(snapshot, { object: { value: 1 }, array: [1, 2] });
  assert.equal(Object.getPrototypeOf(snapshot), Object.prototype);
  assert.equal(Object.getPrototypeOf(snapshot.object), Object.prototype);
  assert.equal(Object.getPrototypeOf(snapshot.array), Array.prototype);

  assert.equal(isCoreValue(runInNewContext('new (class Example { constructor() { this.x = 1 } })()')), false);
  assert.equal(isCoreValue(runInNewContext('new (class Values extends Array {})(1, 2)')), false);
  assert.equal(isCoreValue(new (class LocalExample { x = 1 })()), false);
  assert.equal(isCoreValue(new (class LocalValues extends Array {})(1, 2)), false);
});

test('plain-container detection does not read mutable prototype constructors', () => {
  const objectConstructor = Object.getOwnPropertyDescriptor(Object.prototype, 'constructor');
  const arrayConstructor = Object.getOwnPropertyDescriptor(Array.prototype, 'constructor');
  try {
    Object.defineProperty(Object.prototype, 'constructor', {
      configurable: true,
      get() {
        throw new Error('Object constructor was read');
      },
    });
    Object.defineProperty(Array.prototype, 'constructor', {
      configurable: true,
      get() {
        throw new Error('Array constructor was read');
      },
    });
    const snapshot = snapshotCoreValue({ values: [1, 2] });
    assert.equal(Array.isArray(snapshot.values), true);
    assert.equal(snapshot.values[0], 1);
    assert.equal(snapshot.values[1], 2);
    assert.equal(isCoreValue(new (class Example { value = 1 })()), false);
    assert.equal(isCoreValue(new (class Values extends Array {})(1)), false);
  } finally {
    Object.defineProperty(Object.prototype, 'constructor', objectConstructor);
    Object.defineProperty(Array.prototype, 'constructor', arrayConstructor);
  }
});

test('request snapshots capture each own data property once and discard validated unknown members', () => {
  const reads = new Map();
  const source = {
    toonrpc: '1.0',
    method: 'first',
    params: { value: 1 },
    id: 'call',
    unknown: { checked: true },
  };
  const hostile = new Proxy(source, {
    getOwnPropertyDescriptor(target, key) {
      reads.set(key, (reads.get(key) ?? 0) + 1);
      const descriptor = Reflect.getOwnPropertyDescriptor(target, key);
      if (key === 'method' && reads.get(key) > 1) return { ...descriptor, value: 'later' };
      return descriptor;
    },
  });

  assert.deepEqual(snapshotRequestObject(hostile), {
    toonrpc: '1.0',
    method: 'first',
    params: { value: 1 },
    id: 'call',
  });
  for (const count of reads.values()) assert.equal(count, 1);
});

test('id and params validators implement their exact domains', () => {
  for (const id of [null, 'call', 0, -9007199254740991, 9007199254740991]) {
    assert.equal(isId(id), true);
  }
  for (const id of [true, 1.5, 9007199254740992, [], {}]) {
    assert.equal(isId(id), false);
  }
  assert.equal(isParams([]), true);
  assert.equal(isParams({ named: 1 }), true);
  assert.equal(isParams(null), false);
  assert.equal(isParams('scalar'), false);
});

test('request validation preserves own-member absence', () => {
  const notification = { toonrpc: '1.0', method: 'ping' };
  const nullId = { toonrpc: '1.0', method: 'ping', id: null };
  assert.equal(isRequestObject(notification), true);
  assert.equal(isNotification(notification), true);
  assert.equal(isRequestObject(nullId), true);
  assert.equal(isNotification(nullId), false);
  assert.equal(isRequestObject({ ...notification, params: null }), false);
  assert.equal(isRequestObject({ ...notification, params: undefined }), false);
  assert.equal(isRequestObject({ ...notification, id: undefined }), false);
});

test('response validation uses branch and data presence rather than values', () => {
  assert.equal(isResponse({ toonrpc: '1.0', result: null, id: null }), true);
  assert.equal(
    isResponse({
      toonrpc: '1.0',
      error: { code: 1, message: 'failure', data: null },
      id: 'x',
    }),
    true
  );
  assert.equal(isResponse({ toonrpc: '1.0', result: null, error: null, id: 1 }), false);
  assert.equal(isResponse({ toonrpc: '1.0', id: 1 }), false);
  assert.equal(isResponse({ toonrpc: '1.0', result: true }), false);
  assert.equal(isResponse({ toonrpc: '0.9', result: true, id: 1 }), false);
});

test('Error Objects require string messages, i32 codes and valid optional data', () => {
  assert.equal(isErrorObject({ code: -2147483648, message: 'minimum' }), true);
  assert.equal(isErrorObject({ code: 2147483647, message: 'maximum', data: null }), true);
  assert.equal(isErrorObject({ code: 1.5, message: 'fractional' }), false);
  assert.equal(isErrorObject({ code: -2147483649, message: 'low' }), false);
  assert.equal(isErrorObject({ code: 2147483648, message: 'high' }), false);
  assert.equal(isErrorObject({ code: 1, message: 1 }), false);
  assert.equal(isErrorObject({ code: 1, message: 'bad data', data: undefined }), false);
});
