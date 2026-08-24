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
/** Longest accepted length header: 15 digits keeps the value a safe integer. */
const MAX_LENGTH_DIGITS = 15;
const LF = 0x0a;
const DIGIT_0 = 0x30;
const DIGIT_9 = 0x39;
export class FramingError extends Error {
    constructor(message) {
        super(message);
        this.name = 'FramingError';
    }
}
/** Encode one complete RPC document as a single stream frame. */
export function encodeFrame(document) {
    const header = new TextEncoder().encode(`${document.length}\n`);
    const frame = new Uint8Array(header.length + document.length + 1);
    frame.set(header, 0);
    frame.set(document, header.length);
    frame[frame.length - 1] = LF;
    return frame;
}
/**
 * Incremental decoder: push arbitrary chunk splits in, pull complete
 * documents out. A framing violation throws and poisons the decoder —
 * the stream has no recoverable resynchronization point.
 */
export class FrameDecoder {
    buffer = new Uint8Array(0);
    failure;
    /** Append a chunk and return every document completed by it, in order. */
    push(chunk) {
        if (this.failure)
            throw this.failure;
        this.buffer = concat(this.buffer, chunk);
        const documents = [];
        for (;;) {
            const parsed = this.takeFrame();
            if (!parsed)
                return documents;
            documents.push(parsed);
        }
    }
    /** True when a partially received frame is still buffered. */
    get hasPartialFrame() {
        return this.buffer.length > 0;
    }
    /** Assert the stream ended cleanly on a frame boundary. */
    finish() {
        if (this.failure)
            throw this.failure;
        if (this.buffer.length > 0) {
            throw this.fail('stream ended inside a frame');
        }
    }
    takeFrame() {
        const headerEnd = this.buffer.indexOf(LF);
        if (headerEnd === -1) {
            if (this.buffer.length > MAX_LENGTH_DIGITS) {
                throw this.fail('frame length header is not terminated');
            }
            return undefined;
        }
        if (headerEnd === 0)
            throw this.fail('frame length is empty');
        if (headerEnd > MAX_LENGTH_DIGITS) {
            throw this.fail('frame length header is too long');
        }
        let length = 0;
        for (let i = 0; i < headerEnd; i += 1) {
            const byte = this.buffer[i];
            if (byte < DIGIT_0 || byte > DIGIT_9) {
                throw this.fail('frame length is not a decimal integer');
            }
            length = length * 10 + (byte - DIGIT_0);
        }
        if (headerEnd > 1 && this.buffer[0] === DIGIT_0) {
            throw this.fail('frame length has a leading zero');
        }
        const frameEnd = headerEnd + 1 + length;
        if (this.buffer.length <= frameEnd)
            return undefined;
        if (this.buffer[frameEnd] !== LF) {
            throw this.fail('frame payload is not terminated');
        }
        const document = this.buffer.slice(headerEnd + 1, frameEnd);
        this.buffer = this.buffer.slice(frameEnd + 1);
        return document;
    }
    fail(detail) {
        this.failure = new FramingError(`Invalid TOON-RPC stream frame: ${detail}`);
        return this.failure;
    }
}
function concat(left, right) {
    if (left.length === 0)
        return right instanceof Uint8Array ? new Uint8Array(right) : right;
    const merged = new Uint8Array(left.length + right.length);
    merged.set(left, 0);
    merged.set(right, left.length);
    return merged;
}
//# sourceMappingURL=framing.js.map