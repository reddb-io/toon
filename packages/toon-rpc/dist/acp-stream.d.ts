/**
 * dualDialectStream — a drop-in for ACP's `ndJsonStream` that carries JSON-RPC
 * and TOON-RPC on the same byte stream.
 *
 * The ACP SDK's `Stream` is a pair of object streams: the SDK never sees wire
 * bytes, only decoded messages. This codec owns the bytes and applies three
 * rules, taken from the resident-wire migration that proved them:
 *
 * 1. **Every frame is sniffed on its own bytes.** A frame opening with `{` or
 *    `[` is one line of JSON — a `[` can only be a JSON-RPC batch, which the
 *    stock SDK emits by default; anything else is a TOON document terminated
 *    by a blank line (the TOON encoder escapes `\n` inside strings, so a blank
 *    line can only occur between documents). No peer is told, asked, or
 *    configured.
 * 2. **The consumer always sees `jsonrpc: "2.0"` objects.** On the TOON wire
 *    the envelope field is `toonrpc: "1.0"`; the codec rewrites it at the
 *    boundary in both directions, so an unmodified JSON-RPC stack (e.g. an ACP
 *    connection) rides either dialect. A batch passes through as an array with
 *    each element normalized.
 * 3. **Writes answer in kind.** The peer's dialect is latched only when a
 *    frame actually DECODED in it — never on the framing sniff alone. Until
 *    that proof, writes use `preferred`, whose default `"jsonrpc"` is the only
 *    opener that is safe against a stock JSON-RPC peer. Setting
 *    `preferred: "toonrpc"` opens the conversation in TOON and is only sound
 *    against peers known to read TOON-RPC (a closed deployment); a negotiated
 *    downgrade proof for open systems is tracked by the 0.30 recovery.
 *
 * **Behavioral parity with `ndJsonStream` is the contract**: a malformed frame
 * is reported through `onDiagnostic` and skipped, never a torn-down
 * connection; the final unterminated frame at end of input is flushed; and
 * cancelling the readable releases the underlying byte reader.
 */
type Protocol = 'jsonrpc' | 'toonrpc';
export interface DualDialectDiagnostic {
    reason: 'skipped-frame' | 'dialect-change';
    dialect: Protocol;
    /** For skipped frames: the first 200 characters of the offending frame. */
    frame?: string;
    error?: unknown;
}
export interface DualDialectOptions {
    /**
     * Dialect written before the peer has proven one. Default `"jsonrpc"` — the
     * only opener safe against a stock JSON-RPC peer. `"toonrpc"` is an explicit
     * opt-in for closed deployments whose peers are known to read TOON-RPC.
     */
    preferred?: Protocol;
    /** Receives skipped-frame and dialect-change reports. Default: console.error. */
    onDiagnostic?: (diagnostic: DualDialectDiagnostic) => void;
}
export interface DualDialectStream {
    /** Outgoing RPC messages written by this side of the connection. */
    writable: WritableStream<Record<string, unknown>>;
    /** Incoming RPC messages read by this side of the connection. */
    readable: ReadableStream<Record<string, unknown>>;
}
/**
 * Create a Stream (the ACP SDK shape) over raw byte streams, speaking both
 * dialects. Signature-compatible with `ndJsonStream(output, input)`.
 */
export declare function dualDialectStream(output: WritableStream<Uint8Array>, input: ReadableStream<Uint8Array>, options?: DualDialectOptions): DualDialectStream;
export {};
//# sourceMappingURL=acp-stream.d.ts.map