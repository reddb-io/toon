/**
 * Length-prefixed stream framing for TOON-RPC byte-stream transports.
 *
 * The core protocol operates on complete RPC documents, and byte streams
 * (TCP, Unix sockets, stdio) carry no document boundaries of their own. This
 * profile makes the boundary explicit instead of inferring it from newlines,
 * which a multi-line TOON document cannot guarantee to be free of:
 *
 *     frame = length , LF , payload , LF
 *
 * where `length` is the payload size in bytes as ASCII decimal digits with no
 * sign or leading zeros (a lone `0` is valid), LF is byte 0x0A, and `payload`
 * is exactly `length` bytes of one complete RPC document. The trailing LF is
 * a frame terminator, not part of the payload. Any deviation — a non-digit in
 * the length, a missing terminator, a length too large to represent — is a
 * framing error, and a decoder MUST fail the stream rather than resynchronize.
 */
export declare class FramingError extends Error {
    constructor(message: string);
}
/** Encode one complete RPC document as a single stream frame. */
export declare function encodeFrame(document: Uint8Array): Uint8Array;
/**
 * Incremental decoder: push arbitrary chunk splits in, pull complete
 * documents out. A framing violation throws and poisons the decoder —
 * the stream has no recoverable resynchronization point.
 */
export declare class FrameDecoder {
    private buffer;
    private failure;
    /** Append a chunk and return every document completed by it, in order. */
    push(chunk: Uint8Array): Uint8Array[];
    /** True when a partially received frame is still buffered. */
    get hasPartialFrame(): boolean;
    /** Assert the stream ended cleanly on a frame boundary. */
    finish(): void;
    private takeFrame;
    private fail;
}
//# sourceMappingURL=framing.d.ts.map