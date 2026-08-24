import type { CoreValue } from './protocol.js';
export declare class RpcError extends Error {
    readonly code: number;
    readonly data: CoreValue | undefined;
    readonly hasData: boolean;
    constructor(code: number, message: string);
    constructor(code: number, message: string, data: CoreValue);
}
//# sourceMappingURL=rpc-error.d.ts.map