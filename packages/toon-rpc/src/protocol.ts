export const TOONRPC_VERSION = '1.0';

export type CorePrimitive = null | boolean | number | string;
export type CoreObject = { [key: string]: CoreValue };
export type CoreArray = readonly CoreValue[];
export type CoreValue = CorePrimitive | CoreObject | CoreArray;
export type Id = string | number | null;
export type Params = CoreArray | CoreObject;

export interface Request {
  toonrpc: '1.0';
  method: string;
  params?: Params;
  id: Id;
}

export interface Notification {
  toonrpc: '1.0';
  method: string;
  params?: Params;
  id?: never;
}

export type RequestObject = Request | Notification;

export interface ResponseSuccess {
  toonrpc: '1.0';
  result: CoreValue;
  id: Id;
}

export interface ResponseError {
  toonrpc: '1.0';
  error: ErrorObject;
  id: Id;
}

/** A response is exactly one of success or error, never both or neither. */
export type Response = ResponseSuccess | ResponseError;

export interface ErrorObject {
  code: number;
  message: string;
  /** Runtime validation rejects an own `data: undefined` member. */
  data?: CoreValue;
}

interface ValueTask {
  kind: 'value';
  value: unknown;
  assign: (value: CoreValue) => void;
}

interface LeaveTask {
  kind: 'leave';
  value: object;
}

type SnapshotTask = ValueTask | LeaveTask;

interface SnapshotBudget {
  remaining: number;
}

const arrayIsArray = Array.isArray;
const defineProperty = Object.defineProperty;
const getOwnPropertyDescriptor = Object.getOwnPropertyDescriptor;
const getPrototypeOf = Object.getPrototypeOf;
const hasOwn = Object.hasOwn;
const ownKeys = Reflect.ownKeys;
const objectPrototypeKeys = ownKeys(Object.prototype);

// Configurable limits are part of the later protocol-limits slice.
const MAX_SNAPSHOT_NODES = 25_000;

export function isUnicodeScalarString(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      return false;
    }
  }
  return true;
}

function isCoreNumber(value: number): boolean {
  return Number.isFinite(value) && (!Number.isInteger(value) || Number.isSafeInteger(value));
}

function hasPrimordialObjectKeys(prototype: object): boolean {
  const keys = ownKeys(prototype);
  return (
    keys.length === objectPrototypeKeys.length &&
    objectPrototypeKeys.every((expected) => keys.some((key) => key === expected))
  );
}

function isIntrinsicObjectPrototype(prototype: object): boolean {
  return getPrototypeOf(prototype) === null && hasPrimordialObjectKeys(prototype);
}

function isPlainArray(value: object): boolean {
  const prototype = getPrototypeOf(value);
  if (!arrayIsArray(prototype)) return false;
  const objectPrototype = getPrototypeOf(prototype);
  return objectPrototype !== null && isIntrinsicObjectPrototype(objectPrototype);
}

function isPlainObject(value: object): boolean {
  const prototype = getPrototypeOf(value);
  if (prototype === null) return true;
  return isIntrinsicObjectPrototype(prototype);
}

function define(target: CoreObject | CoreValue[], key: string | number, value: CoreValue): void {
  defineProperty(target, key, {
    value,
    enumerable: true,
    configurable: true,
    writable: true,
  });
}

function reserveExpansion(budget: SnapshotBudget): boolean {
  if (budget.remaining === 0) return false;
  budget.remaining -= 1;
  return true;
}

function inspectArray(
  source: object,
  budget: SnapshotBudget
): { target: CoreValue[]; children: Array<[number, unknown]> } | undefined {
  if (!isPlainArray(source)) return undefined;
  // A single ownKeys trap is atomic; budgeting can only bound work after it returns.
  const keys = ownKeys(source);
  const lengthDescriptor = getOwnPropertyDescriptor(source, 'length');
  if (!lengthDescriptor || !('value' in lengthDescriptor) || typeof lengthDescriptor.value !== 'number') {
    return undefined;
  }
  const length = lengthDescriptor.value;
  const children: Array<[number, unknown]> = [];
  for (const key of keys) {
    if (key === 'length') continue;
    if (typeof key !== 'string' || !/^(0|[1-9]\d*)$/.test(key)) return undefined;
    const index = Number(key);
    if (!Number.isSafeInteger(index) || index >= length) return undefined;
    if (!reserveExpansion(budget)) return undefined;
    const descriptor = getOwnPropertyDescriptor(source, key);
    if (!descriptor?.enumerable || !('value' in descriptor)) return undefined;
    children.push([index, descriptor.value]);
  }
  if (children.length !== length) return undefined;
  children.sort(([left], [right]) => left - right);
  if (children.some(([index], position) => index !== position)) return undefined;
  return { target: new Array<CoreValue>(length), children };
}

function inspectObject(
  source: object,
  omitUndefinedProperties: boolean,
  budget: SnapshotBudget
): { target: CoreObject; children: Array<[string, unknown]> } | undefined {
  if (!isPlainObject(source)) return undefined;
  const children: Array<[string, unknown]> = [];
  // A single ownKeys trap is atomic; budgeting can only bound work after it returns.
  for (const key of ownKeys(source)) {
    if (typeof key !== 'string' || !isUnicodeScalarString(key)) return undefined;
    if (!reserveExpansion(budget)) return undefined;
    const descriptor = getOwnPropertyDescriptor(source, key);
    if (!descriptor?.enumerable || !('value' in descriptor)) return undefined;
    if (omitUndefinedProperties && descriptor.value === undefined) continue;
    children.push([key, descriptor.value]);
  }
  return { target: {}, children };
}

function snapshotCoreValueInternal(
  value: unknown,
  omitUndefinedProperties: boolean
): CoreValue | undefined {
  let result: CoreValue | undefined;
  const active = new WeakSet<object>();
  const budget: SnapshotBudget = { remaining: MAX_SNAPSHOT_NODES };
  if (!reserveExpansion(budget)) return undefined;
  const tasks: SnapshotTask[] = [{ kind: 'value', value, assign: (next) => (result = next) }];

  try {
    while (tasks.length > 0) {
      const task = tasks.pop()!;
      if (task.kind === 'leave') {
        active.delete(task.value);
        continue;
      }

      const current = task.value;
      if (current === null || typeof current === 'boolean') {
        task.assign(current);
      } else if (typeof current === 'string') {
        if (!isUnicodeScalarString(current)) return undefined;
        task.assign(current);
      } else if (typeof current === 'number') {
        if (!isCoreNumber(current)) return undefined;
        task.assign(current);
      } else if (typeof current === 'object') {
        if (active.has(current)) return undefined;
        active.add(current);
        const inspected = arrayIsArray(current)
          ? inspectArray(current, budget)
          : inspectObject(current, omitUndefinedProperties, budget);
        if (!inspected) return undefined;

        task.assign(inspected.target);
        tasks.push({ kind: 'leave', value: current });
        for (let index = inspected.children.length - 1; index >= 0; index -= 1) {
          const [key, child] = inspected.children[index]!;
          tasks.push({
            kind: 'value',
            value: child,
            assign: (next, target = inspected.target, property = key) =>
              define(target, property, next),
          });
        }
      } else {
        return undefined;
      }
    }
  } catch {
    return undefined;
  }
  return result;
}

/** Validate and copy a core value without getters or later reads from the source. */
export function snapshotCoreValue(value: unknown): CoreValue | undefined {
  return snapshotCoreValueInternal(value, false);
}

/** MCP boundary helper: omit undefined object properties but never array elements. */
export function snapshotCoreValueOmittingUndefinedProperties(
  value: unknown
): CoreValue | undefined {
  return snapshotCoreValueInternal(value, true);
}

export function snapshotParams(value: unknown): Params | undefined {
  const snapshot = snapshotCoreValue(value);
  return snapshot !== undefined && snapshot !== null && typeof snapshot === 'object'
    ? snapshot
    : undefined;
}

export function snapshotRequestObject(value: unknown): RequestObject | undefined {
  const snapshot = snapshotCoreValue(value);
  if (snapshot === undefined || snapshot === null || typeof snapshot !== 'object' || arrayIsArray(snapshot)) {
    return undefined;
  }
  const record = snapshot as CoreObject;
  if (!hasOwn(record, 'toonrpc') || record.toonrpc !== TOONRPC_VERSION) return undefined;
  if (!hasOwn(record, 'method') || typeof record.method !== 'string' || record.method.length === 0) {
    return undefined;
  }
  if (hasOwn(record, 'params') && snapshotParams(record.params) === undefined) return undefined;
  if (hasOwn(record, 'id') && !isId(record.id)) return undefined;

  const envelope: Notification | Request = {
    toonrpc: TOONRPC_VERSION,
    method: record.method,
    ...(hasOwn(record, 'params') ? { params: record.params as Params } : {}),
    ...(hasOwn(record, 'id') ? { id: record.id as Id } : {}),
  };
  return envelope;
}

export function snapshotErrorObject(value: unknown): ErrorObject | undefined {
  const snapshot = snapshotCoreValue(value);
  if (snapshot === undefined || snapshot === null || typeof snapshot !== 'object' || arrayIsArray(snapshot)) {
    return undefined;
  }
  const record = snapshot as CoreObject;
  if (!hasOwn(record, 'code') || !hasOwn(record, 'message')) return undefined;
  if (
    typeof record.code !== 'number' ||
    !Number.isInteger(record.code) ||
    record.code < -2147483648 ||
    record.code > 2147483647 ||
    typeof record.message !== 'string'
  ) {
    return undefined;
  }
  return {
    code: record.code,
    message: record.message,
    ...(hasOwn(record, 'data') ? { data: record.data } : {}),
  };
}

export function snapshotResponse(value: unknown): Response | undefined {
  const snapshot = snapshotCoreValue(value);
  if (snapshot === undefined || snapshot === null || typeof snapshot !== 'object' || arrayIsArray(snapshot)) {
    return undefined;
  }
  const record = snapshot as CoreObject;
  if (!hasOwn(record, 'toonrpc') || record.toonrpc !== TOONRPC_VERSION) return undefined;
  if (!hasOwn(record, 'id') || !isId(record.id)) return undefined;
  const hasResult = hasOwn(record, 'result');
  const hasError = hasOwn(record, 'error');
  if (hasResult === hasError) return undefined;
  if (hasResult) {
    return { toonrpc: TOONRPC_VERSION, result: record.result!, id: record.id };
  }
  const error = snapshotErrorObject(record.error);
  return error ? { toonrpc: TOONRPC_VERSION, error, id: record.id } : undefined;
}

export function isCoreValue(value: unknown): value is CoreValue {
  return snapshotCoreValue(value) !== undefined;
}

export function isId(value: unknown): value is Id {
  return (
    value === null ||
    (typeof value === 'string' && isUnicodeScalarString(value)) ||
    (typeof value === 'number' && Number.isSafeInteger(value))
  );
}

export function isParams(value: unknown): value is Params {
  return snapshotParams(value) !== undefined;
}

export function isRequestObject(value: unknown): value is RequestObject {
  return snapshotRequestObject(value) !== undefined;
}

export function isNotification(value: RequestObject): value is Notification {
  try {
    return !hasOwn(value, 'id');
  } catch {
    return false;
  }
}

export function isErrorObject(value: unknown): value is ErrorObject {
  return snapshotErrorObject(value) !== undefined;
}

export function isResponse(value: unknown): value is Response {
  return snapshotResponse(value) !== undefined;
}
