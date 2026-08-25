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

import { encode, decode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';
import { TOONRPC_VERSION } from './index.js';

const JSONRPC_VERSION = '2.0';
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
export function dualDialectStream(
  output: WritableStream<Uint8Array>,
  input: ReadableStream<Uint8Array>,
  options?: DualDialectOptions
): DualDialectStream {
  let peerDialect: Protocol | undefined;
  const preferred: Protocol = options?.preferred ?? 'jsonrpc';
  const report =
    options?.onDiagnostic ??
    ((diagnostic: DualDialectDiagnostic) => {
      if (diagnostic.reason === 'skipped-frame') {
        console.error(
          `dualDialectStream: skipping an unreadable ${diagnostic.dialect} frame (${
            diagnostic.error instanceof Error ? diagnostic.error.message : String(diagnostic.error)
          }): ${diagnostic.frame ?? ''}`
        );
      } else {
        console.error(`dualDialectStream: peer dialect is now ${diagnostic.dialect}`);
      }
    });
  const textEncoder = new TextEncoder();
  const writer = output.getWriter();

  const writable = new WritableStream<Record<string, unknown>>({
    async write(message) {
      const dialect = peerDialect ?? preferred;
      await writer.write(textEncoder.encode(encodeFrame(message, dialect)));
    },
    async close() {
      try {
        await writer.close();
      } finally {
        writer.releaseLock();
      }
    },
    async abort(reason) {
      try {
        await writer.abort(reason);
      } finally {
        writer.releaseLock();
      }
    },
  });

  let buffer = '';
  const reader = input.getReader();

  const decodeAndEnqueue = (
    controller: ReadableStreamDefaultController<Record<string, unknown>>,
    frame: string,
    dialect: Protocol
  ): void => {
    let decoded: unknown;
    try {
      decoded = decodeFrameStrict(frame, dialect);
    } catch (error) {
      // Parity with ndJsonStream: one unreadable frame is reported and
      // skipped; it never tears down the connection.
      report({ reason: 'skipped-frame', dialect, frame: frame.slice(0, 200), error });
      return;
    }
    // The latch moves only on proof — a frame that decoded. A garbage frame
    // that merely failed to open with `{` must never flip a JSON-only peer
    // into receiving TOON it cannot read.
    if (peerDialect !== dialect) {
      if (peerDialect !== undefined) report({ reason: 'dialect-change', dialect });
      peerDialect = dialect;
    }
    controller.enqueue(normalizeInbound(decoded));
  };

  const readable = new ReadableStream<Record<string, unknown>>({
    async start(controller) {
      const textDecoder = new TextDecoder();
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += textDecoder.decode(value, { stream: true });
          for (const { frame, dialect } of drainFrames()) {
            decodeAndEnqueue(controller, frame, dialect);
          }
        }
        // Parity with ndJsonStream's flush: a final frame written without its
        // terminator before close is still a frame, not silence.
        const remainder = buffer.trim();
        if (remainder !== '') {
          decodeAndEnqueue(controller, remainder, sniffDialect(remainder));
        }
        controller.close();
      } catch (err) {
        controller.error(err);
      } finally {
        reader.releaseLock();
      }
    },
    async cancel(reason) {
      // The SDK cancels its reader on connection close; without this the
      // underlying byte reader is never released and the descriptor outlives
      // every session it served.
      await reader.cancel(reason).catch(() => undefined);
    },
  });

  /** Take every complete frame out of `buffer`, leaving a partial tail. */
  function* drainFrames(): Generator<{ frame: string; dialect: Protocol }> {
    while (true) {
      // Leading newlines are inter-frame padding, never content.
      const start = buffer.search(/[^\r\n]/);
      if (start < 0) {
        buffer = '';
        return;
      }
      buffer = buffer.slice(start);

      if (sniffDialect(buffer) === 'jsonrpc') {
        // JSON frame: one line. JSON.stringify never emits a raw newline.
        const end = buffer.indexOf('\n');
        if (end < 0) return;
        const frame = buffer.slice(0, end).replace(/\r$/, '');
        buffer = buffer.slice(end + 1);
        yield { frame, dialect: 'jsonrpc' };
        continue;
      }

      // TOON frame: terminated by a blank line.
      const end = buffer.search(/\r?\n\r?\n/);
      if (end < 0) return;
      const frame = buffer.slice(0, end);
      buffer = buffer.slice(end).replace(/^\r?\n\r?\n/, '');
      yield { frame, dialect: 'toonrpc' };
    }
  }

  return { writable, readable };
}

/**
 * The dialect a frame is written in, decided by its first byte. `{` or `[`
 * means one JSON line — a `[` can only be a JSON-RPC batch, and a TOON read
 * of one would wait forever for a blank-line terminator, wedging every frame
 * behind it. Anything else is a TOON document.
 */
function sniffDialect(text: string): Protocol {
  const head = text[0];
  return head === '{' || head === '[' ? 'jsonrpc' : 'toonrpc';
}

/**
 * Encode one outgoing message, terminator included. A top-level array — a
 * JSON-RPC batch — is always one JSON line in either dialect: the sniff reads
 * `[` as JSON, and TOON cannot carry a top-level array as one document. A
 * value the TOON encoder refuses falls back to a JSON line, which rule 1
 * guarantees every reader on this wire accepts.
 */
function encodeFrame(message: Record<string, unknown>, dialect: Protocol): string {
  if (Array.isArray(message)) return `${JSON.stringify(message)}\n`;
  const { jsonrpc: _j, toonrpc: _t, ...rest } = message as Record<string, JsonValue>;
  if (dialect === 'toonrpc') {
    const body = tryEncodeToon({ toonrpc: TOONRPC_VERSION, ...rest });
    if (body !== null) return body.endsWith('\n') ? `${body}\n` : `${body}\n\n`;
  }
  return `${JSON.stringify({ jsonrpc: JSONRPC_VERSION, ...rest })}\n`;
}

function tryEncodeToon(value: Record<string, JsonValue>): string | null {
  try {
    // The JSON round trip drops `undefined` members and applies toJSON, which
    // the TOON encoder refuses rather than silently omits.
    const json = JSON.stringify(value);
    if (json === undefined) return null;
    const body = encode(JSON.parse(json) as JsonValue);
    return body.trim() === '' ? null : body;
  } catch {
    return null;
  }
}

/**
 * Decode one frame in exactly the dialect its bytes were sniffed as. The TOON
 * decoder is lenient enough to "succeed" on some foreign input, so it must
 * never see a frame the sniff called JSON, and vice versa.
 */
function decodeFrameStrict(frame: string, dialect: Protocol): unknown {
  return dialect === 'jsonrpc' ? (JSON.parse(frame) as unknown) : (decode(frame) as unknown);
}

/**
 * The envelope the SDK expects, whatever the wire wore. A batch passes
 * through as an array with each element normalized.
 */
function normalizeInbound(decoded: unknown): Record<string, unknown> {
  if (Array.isArray(decoded)) {
    return decoded.map((element) => normalizeInbound(element)) as unknown as Record<
      string,
      unknown
    >;
  }
  if (typeof decoded !== 'object' || decoded === null) {
    return { jsonrpc: JSONRPC_VERSION, invalid: decoded };
  }
  const { jsonrpc: _j, toonrpc: _t, ...rest } = decoded as Record<string, unknown>;
  return { jsonrpc: JSONRPC_VERSION, ...rest };
}
