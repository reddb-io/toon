//! MCP stdio transport: one JSON-RPC message per line.
//!
//! Framing follows the stdio binding of the pinned revision:
//!
//! - the server reads JSON-RPC messages from stdin, one per line;
//! - it writes JSON-RPC messages to stdout, one per line, never containing an
//!   embedded newline;
//! - nothing that is not a valid MCP message is written to stdout, so all
//!   logging goes to stderr;
//! - EOF on stdin is the graceful shutdown signal, and the server exits.

use crate::dispatcher::McpDispatcher;
use crate::McpService;
use std::io::{self, BufRead, Write};
use std::sync::Arc;

/// Serve MCP over stdin/stdout until EOF.
pub fn serve_stdio<S: McpService>(service: S) -> io::Result<()> {
    let dispatcher = McpDispatcher::new(Arc::new(service));
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve_stdio_with(&dispatcher, &mut stdin.lock(), &mut stdout.lock())
}

/// Serve MCP over arbitrary line-oriented streams.
///
/// The framing is not specific to the standard streams, so the same loop drives
/// a Unix socket, a TCP connection, or an in-memory buffer in tests.
pub fn serve_stdio_with<S: McpService, R: BufRead, W: Write>(
    dispatcher: &McpDispatcher<S>,
    input: &mut R,
    output: &mut W,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if let Some(response) = dispatcher.handle_line(&line) {
            debug_assert!(
                !response.contains('\n'),
                "a stdio message must not contain an embedded newline"
            );
            output.write_all(response.as_bytes())?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}
