/**
 * dualDialectStream — a drop-in for ACP's `ndJsonStream` that carries JSON-RPC
 * and TOON-RPC on the same byte stream.
 *
 * The ACP SDK's `Stream` is a pair of object streams: the SDK never sees wire
 * bytes, only decoded messages. This codec owns the bytes and applies three
 * rules, taken from the resident-wire migration that proved them:
 *
 * 1. **Every frame is sniffed on its own bytes.** A frame opening with `{` is
 *    one line of JSON; anything else is a TOON document terminated by a blank
 *    line (the TOON encoder escapes `\n` inside strings, so a blank line can
 *    only occur between documents). No peer is told, asked, or configured.
 * 2. **The consumer always sees `jsonrpc: "2.0"` objects.** On the TOON wire
 *    the envelope field is `toonrpc: "1.0"`; the codec rewrites it at the
 *    boundary in both directions, so an unmodified JSON-RPC stack (e.g. an ACP
 *    connection) rides either dialect.
 * 3. **Writes answer in kind.** Until the peer has proven a dialect by sending
 *    a frame, writes use `preferred` (default `"json"`, the maximally
 *    compatible opener); after that, writes follow the peer.
 */
type Protocol = 'jsonrpc' | 'toonrpc';
export interface DualDialectOptions {
    /** Dialect written before the peer has proven one. Default `"json"`. */
    preferred?: Protocol;
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