import { RpcError } from '../dist/index.js';
import type { CoreArray, CoreValue, ErrorObject, MethodHandler, Params } from '../dist/index.js';

new RpcError(1, 'without data');
new RpcError(1, 'with null data', null);
new RpcError(1, 'with object data', { value: true });
const readonlyArray = [{ value: true }, null] as const;
const coreArray: CoreArray = readonlyArray;
const coreValue: CoreValue = readonlyArray;
const params: Params = readonlyArray;
new RpcError(1, 'with readonly data', readonlyArray);

// @ts-expect-error An explicitly present undefined is not core data.
new RpcError(1, 'undefined data', undefined);

// @ts-expect-error Core objects cannot contain undefined values.
const invalidCore: CoreValue = { value: undefined };

const handler: MethodHandler = async (params: Params | undefined): Promise<CoreValue> =>
  params ?? null;
const error: ErrorObject = { code: 1, message: 'application error', data: null };

void invalidCore;
void handler;
void error;
void coreArray;
void coreValue;
void params;
