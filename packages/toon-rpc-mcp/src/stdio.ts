/**
 * MCP stdio transport — reads newline-delimited TOON from stdin, writes to stdout.
 */

import type { Server } from '@reddb-io/toon-rpc';

export function serveStdio(server: Server): void {
  let buffer = '';

  process.stdin.setEncoding('utf8');
  process.stdin.on('data', (chunk: string) => {
    buffer += chunk;
    let idx;
    while ((idx = buffer.indexOf('\n\n')) !== -1) {
      const msg = buffer.slice(0, idx);
      buffer = buffer.slice(idx + 2);
      if (!msg.trim()) continue;

      // The dispatcher returns a Promise; we synchronously handle it.
      handleMessage(server, msg).catch((err) => {
        process.stderr.write(`[toon-rpc-mcp] error: ${err}\n`);
      });
    }
  });

  process.stdin.on('end', () => {
    process.exit(0);
  });

  process.stderr.write('[toon-rpc-mcp] stdio server ready\n');
}

async function handleMessage(server: Server, msg: string): Promise<void> {
  const response = await server.handleText(msg);
  if (response && response.length > 0) {
    const text = new TextDecoder().decode(response);
    process.stdout.write(text + '\n\n');
  }
}
