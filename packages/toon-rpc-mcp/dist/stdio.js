/**
 * MCP stdio transport: one JSON-RPC message per line.
 *
 * Framing follows the stdio binding of the pinned revision:
 *
 * - messages are read from stdin, one per line;
 * - messages are written to stdout, one per line, never containing an embedded
 *   newline;
 * - nothing that is not a valid MCP message reaches stdout, so logging goes to
 *   stderr;
 * - EOF on stdin is the graceful shutdown signal.
 */
/** Serve MCP over stdin/stdout until EOF. */
export function serveStdio(dispatcher) {
    serveStdioWith(dispatcher, { input: process.stdin, output: process.stdout });
    process.stderr.write('[toon-rpc-mcp] stdio server ready\n');
}
/**
 * Serve MCP over arbitrary line-oriented streams.
 *
 * Resolves once the input stream ends, so tests can await a full transcript.
 * Responses are written in request order: each line is awaited before the next
 * is dispatched, so a slow call cannot reorder the stream.
 */
export function serveStdioWith(dispatcher, streams) {
    const { input, output } = streams;
    return new Promise((resolve, reject) => {
        let buffer = '';
        // Serializes dispatch so responses keep request order.
        let queue = Promise.resolve();
        const dispatch = (line) => {
            queue = queue.then(async () => {
                try {
                    const response = await dispatcher.handleLine(line);
                    if (response !== null)
                        output.write(response + '\n');
                }
                catch (e) {
                    process.stderr.write(`[toon-rpc-mcp] dispatch error: ${e.message}\n`);
                }
            });
        };
        input.setEncoding('utf8');
        input.on('data', (chunk) => {
            buffer += typeof chunk === 'string' ? chunk : chunk.toString('utf8');
            let index;
            while ((index = buffer.indexOf('\n')) !== -1) {
                const line = buffer.slice(0, index);
                buffer = buffer.slice(index + 1);
                dispatch(line);
            }
        });
        input.on('end', () => {
            // A final line without a trailing newline is still a message.
            if (buffer.trim() !== '')
                dispatch(buffer);
            buffer = '';
            queue.then(resolve, reject);
        });
        input.on('error', reject);
    });
}
//# sourceMappingURL=stdio.js.map