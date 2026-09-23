/**
 * The MCP stdio transport: newline-delimited JSON-RPC messages on stdin and
 * stdout, one message per line with no embedded newlines. Anything else the
 * server prints must go to stderr.
 *
 * Requests are answered concurrently, as MCP allows, with at most
 * `maxPendingCalls` in flight; past that, and past a line longer than
 * `maxFrameBytes`, the stream is ended with a JSON-RPC error.
 */
import { DEFAULT_LIMITS } from '@reddb-io/toon-rpc';
import { McpServer } from './index.js';
/** Serve one MCP session over stdio; resolves once input ends and every answer is written. */
export function serveStdio(service, options = {}) {
    const input = options.input ?? process.stdin;
    const output = options.output ?? process.stdout;
    const maxLine = options.limits?.maxFrameBytes ?? DEFAULT_LIMITS.maxFrameBytes;
    const maxInFlight = options.limits?.maxPendingCalls ?? DEFAULT_LIMITS.maxPendingCalls;
    const server = new McpServer(service);
    const inFlight = new Set();
    let buffer = '';
    let stopped = false;
    const write = (line) => {
        if (!output.writableEnded)
            output.write(`${line}\n`);
    };
    const stop = (message) => {
        stopped = true;
        write(JSON.stringify({ jsonrpc: '2.0', id: null, error: { code: -32600, message } }));
        input.destroy();
    };
    const answer = (line) => {
        if (inFlight.size >= maxInFlight)
            return stop('Too many requests in flight');
        const pending = server.handleLine(line).then((response) => {
            if (response !== undefined)
                write(response);
        });
        inFlight.add(pending);
        void pending.finally(() => inFlight.delete(pending));
    };
    return new Promise((resolve) => {
        let finishing;
        const finish = () => (finishing ??= (async () => {
            while (inFlight.size > 0)
                await Promise.all(inFlight);
            output.end(resolve);
        })());
        input.setEncoding('utf8');
        input.on('data', (chunk) => {
            if (stopped)
                return;
            buffer += chunk;
            for (let end = buffer.indexOf('\n'); end !== -1; end = buffer.indexOf('\n')) {
                const line = buffer.slice(0, end).replace(/\r$/, '');
                buffer = buffer.slice(end + 1);
                if (line.trim() !== '')
                    answer(line);
                if (stopped)
                    return;
            }
            if (buffer.length > maxLine)
                stop('Message exceeds the size limit');
        });
        input.on('end', () => {
            if (!stopped && buffer.trim() !== '')
                answer(buffer);
            void finish();
        });
        input.on('close', () => void finish());
    });
}
//# sourceMappingURL=stdio.js.map