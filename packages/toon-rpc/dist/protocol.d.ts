export declare const TOONRPC_VERSION = "1.0";
export type CorePrimitive = null | boolean | number | string;
export type CoreObject = {
    [key: string]: CoreValue;
};
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
export declare function isUnicodeScalarString(value: string): boolean;
/** Validate and copy a core value without getters or later reads from the source. */
export declare function snapshotCoreValue(value: unknown): CoreValue | undefined;
/** MCP boundary helper: omit undefined object properties but never array elements. */
export declare function snapshotCoreValueOmittingUndefinedProperties(value: unknown): CoreValue | undefined;
export declare function snapshotParams(value: unknown): Params | undefined;
export declare function snapshotRequestObject(value: unknown): RequestObject | undefined;
export declare function snapshotErrorObject(value: unknown): ErrorObject | undefined;
export declare function snapshotResponse(value: unknown): Response | undefined;
export declare function isCoreValue(value: unknown): value is CoreValue;
export declare function isId(value: unknown): value is Id;
export declare function isParams(value: unknown): value is Params;
export declare function isRequestObject(value: unknown): value is RequestObject;
export declare function isNotification(value: RequestObject): value is Notification;
export declare function isErrorObject(value: unknown): value is ErrorObject;
export declare function isResponse(value: unknown): value is Response;
//# sourceMappingURL=protocol.d.ts.map