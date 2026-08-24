import type { CoreValue } from './protocol.js';

export class RpcError extends Error {
  readonly code: number;
  readonly data: CoreValue | undefined;
  readonly hasData: boolean;

  constructor(code: number, message: string);
  constructor(code: number, message: string, data: CoreValue);
  constructor(code: number, message: string, data?: CoreValue) {
    super(message);
    this.name = 'RpcError';
    this.code = code;
    this.data = data;
    this.hasData = arguments.length >= 3;
  }
}
