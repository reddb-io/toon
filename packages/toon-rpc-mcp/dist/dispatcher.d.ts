/** MCP method dispatch over JSON-RPC 2.0. */
import type { JsonRpcResponse, JsonValue } from './jsonrpc.js';
import type { McpService } from './types.js';
export interface McpDispatcherOptions {
    /**
     * Also answer the legacy `initialize` handshake of MCP `2025-11-25`, making
     * this a dual-era server. Off by default: a modern-only server rejects
     * `initialize` while naming the versions it does support.
     */
    legacyInitialize?: boolean;
}
export declare class McpDispatcher {
    #private;
    constructor(service: McpService, options?: McpDispatcherOptions);
    get supportedVersions(): string[];
    /**
     * Handle one raw newline-delimited JSON line.
     *
     * Resolves to the response line to write, or `null` for a notification.
     */
    handleLine(line: string): Promise<string | null>;
    /** Handle one decoded message. Returns `null` for a notification. */
    handleMessage(message: unknown): Promise<JsonRpcResponse | null>;
    discover(): Promise<JsonValue>;
}
/** Convenience constructor. */
export declare function createMcpDispatcher(service: McpService, options?: McpDispatcherOptions): McpDispatcher;
//# sourceMappingURL=dispatcher.d.ts.map