/**
 * Resource limits shared by every client, server and transport.
 *
 * The defaults match `crates/reddb-io-toon-rpc/src/limits.rs`. Going past a
 * limit is always visible: a defined RPC error, a refused HTTP request, a
 * rejected call, or a failed transport — never a silently dropped document.
 */
export interface Limits {
    /** Largest stream frame, WebSocket message or SSE event, in bytes. */
    maxFrameBytes: number;
    /** Largest HTTP request (server) or response (client) body, in bytes. */
    maxBodyBytes: number;
    /** Most entries a batch may hold; a longer batch is one Invalid Request. */
    maxBatchLength: number;
    /** Most calls one client keeps pending; the next call is refused. */
    maxPendingCalls: number;
    /** Most connections a server serves at once. */
    maxConnections: number;
    /** Most received documents a transport buffers for its consumer. */
    maxQueuedDocuments: number;
    /** A connection with no incoming document for this long is closed. */
    idleTimeoutMs: number | undefined;
    /** Default timeout for a client call that sets none of its own. */
    requestTimeoutMs: number | undefined;
    /** How long a shutting-down server lets open connections finish. */
    shutdownGraceMs: number;
}
export declare const DEFAULT_LIMITS: Readonly<Limits>;
/** The defaults with `overrides` applied. */
export declare function resolveLimits(overrides?: Partial<Limits>): Limits;
//# sourceMappingURL=limits.d.ts.map