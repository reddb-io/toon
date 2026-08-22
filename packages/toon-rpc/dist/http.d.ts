/**
 * HTTP transport for TOON-RPC
 */
export interface HttpTransportOptions {
    url: string;
    headers?: Record<string, string>;
}
export interface HttpTransport {
    send(data: Uint8Array): Promise<void>;
    recv(): AsyncIterable<Uint8Array>;
    close(): Promise<void>;
}
export declare function createHttpTransport(options: HttpTransportOptions): HttpTransport;
/**
 * HTTP client transport (full request/response)
 */
export declare class HttpClient {
    private url;
    private headers;
    private buffer;
    constructor(url: string, headers?: Record<string, string>);
    send(data: Uint8Array): Promise<void>;
    recv(): Promise<Uint8Array>;
}
//# sourceMappingURL=http.d.ts.map