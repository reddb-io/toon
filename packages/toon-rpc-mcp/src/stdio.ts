/**
 * The MCP stdio transport: newline-delimited JSON-RPC messages on stdin and
 * stdout, one message per line with no embedded newlines. Anything else the
 * server prints must go to stderr.
 *
 * Requests are answered concurrently, as MCP allows, with at most
 * `maxPendingCalls` in flight; past that, and past a line longer than
 * `maxFrameBytes`, the stream is ended with a JSON-RPC error.
 */

import type { Readable, Writable } from 'node:stream';
import { DEFAULT_LIMITS } from '@reddb-io/toon-rpc';
import type { Limits } from '@reddb-io/toon-rpc';
import { McpServer } from './index.js';
import type { McpService } from './index.js';

export interface StdioOptions {
  input?: Readable;
  output?: Writable;
  limits?: Partial<Pick<Limits, 'maxFrameBytes' | 'maxPendingCalls'>>;
}

/** Serve one MCP session over stdio; resolves once input ends and every answer is written. */
export function serveStdio(service: McpService, options: StdioOptions = {}): Promise<void> {
  const input: Readable = options.input ?? process.stdin;
  const output: Writable = options.output ?? process.stdout;
  const maxLine = options.limits?.maxFrameBytes ?? DEFAULT_LIMITS.maxFrameBytes;
  const maxInFlight = options.limits?.maxPendingCalls ?? DEFAULT_LIMITS.maxPendingCalls;
  const server = new McpServer(service);
  const inFlight = new Set<Promise<void>>();
  let buffer = '';
  let stopped = false;

  const write = (line: string) => {
    if (!output.writableEnded) output.write(`${line}\n`);
  };
  const stop = (message: string) => {
    stopped = true;
    write(JSON.stringify({ jsonrpc: '2.0', id: null, error: { code: -32600, message } }));
    input.destroy();
  };
  const answer = (line: string) => {
    if (inFlight.size >= maxInFlight) return stop('Too many requests in flight');
    const pending: Promise<void> = server.handleLine(line).then((response) => {
      if (response !== undefined) write(response);
    });
    inFlight.add(pending);
    void pending.finally(() => inFlight.delete(pending));
  };

  return new Promise<void>((resolve) => {
    let finishing: Promise<void> | undefined;
    const finish = () =>
      (finishing ??= (async () => {
        while (inFlight.size > 0) await Promise.all(inFlight);
        output.end(resolve);
      })());
    input.setEncoding('utf8');
    input.on('data', (chunk: string) => {
      if (stopped) return;
      buffer += chunk;
      for (let end = buffer.indexOf('\n'); end !== -1; end = buffer.indexOf('\n')) {
        const line = buffer.slice(0, end).replace(/\r$/, '');
        buffer = buffer.slice(end + 1);
        if (line.trim() !== '') answer(line);
        if (stopped) return;
      }
      if (buffer.length > maxLine) stop('Message exceeds the size limit');
    });
    input.on('end', () => {
      if (!stopped && buffer.trim() !== '') answer(buffer);
      void finish();
    });
    input.on('close', () => void finish());
  });
}
