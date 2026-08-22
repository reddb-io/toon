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

import { encode, decode } from '@reddb-io/toon';
import type { JsonValue } from '@reddb-io/toon';
import { TOONRPC_VERSION } from './index.js';
import { JSONRPC_VERSION, type Protocol } from './multi.js';

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
export function dualDialectStream(
  output: WritableStream<Uint8Array>,
  input: ReadableStream<Uint8Array>,
  options?: DualDialectOptions
): DualDialectStream {
  let peerDialect: Protocol | undefined;
  const preferred: Protocol = options?.preferred ?? 'jsonrpc';
  const textEncoder = new TextEncoder();
  const writer = output.getWriter();

  const writable = new WritableStream<Record<string, unknown>>({
    async write(message) {
      const dialect = peerDialect ?? preferred;
      const { jsonrpc: _j, toonrpc: _t, ...rest } = message as Record<string, JsonValue>;
      const frame =
        dialect === 'jsonrpc'
          ? `${JSON.stringify({ jsonrpc: JSONRPC_VERSION, ...rest })}\n`
          : `${encode({ toonrpc: TOONRPC_VERSION, ...rest } as JsonValue)}\n\n`;
      await writer.write(textEncoder.encode(frame));
    },
    async close() {
      await writer.close();
    },
    async abort(reason) {
      await writer.abort(reason);
    },
  });

  let buffer = '';
  const readable = new ReadableStream<Record<string, unknown>>({
    async start(controller) {
      const textDecoder = new TextDecoder();
      const reader = input.getReader();
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += textDecoder.decode(value, { stream: true });
          for (const { frame, dialect } of drainFrames()) {
            peerDialect = dialect;
            controller.enqueue(decodeFrame(frame, dialect));
          }
        }
        controller.close();
      } catch (err) {
        controller.error(err);
      } finally {
        reader.releaseLock();
      }
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

      if (buffer.startsWith('{')) {
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

function decodeFrame(frame: string, dialect: Protocol): Record<string, unknown> {
  const raw = (dialect === 'jsonrpc' ? JSON.parse(frame) : decode(frame)) as Record<string, JsonValue>;
  const { jsonrpc: _j, toonrpc: _t, ...rest } = raw;
  return { jsonrpc: JSONRPC_VERSION, ...rest };
}
