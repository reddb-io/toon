/**
 * Resource limits shared by every client, server and transport.
 *
 * The defaults match `crates/reddb-io-toon-rpc/src/limits.rs`. Going past a
 * limit is always visible: a defined RPC error, a refused HTTP request, a
 * rejected call, or a failed transport — never a silently dropped document.
 */
export const DEFAULT_LIMITS = Object.freeze({
    maxFrameBytes: 16 * 1024 * 1024,
    maxBodyBytes: 16 * 1024 * 1024,
    maxBatchLength: 1024,
    maxPendingCalls: 1024,
    maxConnections: 1024,
    maxQueuedDocuments: 1024,
    idleTimeoutMs: 300_000,
    requestTimeoutMs: undefined,
    shutdownGraceMs: 10_000,
});
/** The defaults with `overrides` applied. */
export function resolveLimits(overrides = {}) {
    return { ...DEFAULT_LIMITS, ...overrides };
}
//# sourceMappingURL=limits.js.map