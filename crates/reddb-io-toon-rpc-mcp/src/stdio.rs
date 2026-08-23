//! Experimental stdio transport for the quarantined MCP prototype.
//!
//! Its current blank-line-delimited TOON framing is not MCP-conformant.

use crate::dispatcher::dispatch_mcp;
use crate::McpService;
use std::io::{self, BufRead, Write};
use std::sync::Arc;

/// Run an MCP server over stdio until EOF.
///
/// This entry point is retained for recovery tests only.
/// Logs go to stderr; the wire protocol lives on stdin/stdout only.
pub fn serve_stdio<S: McpService>(service: S) -> io::Result<()> {
    let dispatcher = dispatch_mcp(Arc::new(service));
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    eprintln!("[toon-rpc-mcp] stdio server ready");

    let mut buffer = String::new();
    for line in stdin.lock().lines() {
        let line = line?;

        // Empty line marks the end of a TOON message
        if line.is_empty() {
            if !buffer.is_empty() {
                let response = match dispatcher.dispatch(buffer.trim().as_bytes()) {
                    Ok(bytes) => {
                        let mut s = String::from_utf8(bytes).unwrap_or_default();
                        s.push_str("\n\n");
                        s
                    }
                    Err(e) => {
                        let err = serde_json::json!({
                            "jsonrpc": "2.0",
                            "error": {
                                "code": -32603,
                                "message": e.to_string()
                            },
                            "id": null
                        });
                        let mut s = err.to_string();
                        s.push_str("\n\n");
                        s
                    }
                };
                stdout.write_all(response.as_bytes())?;
                stdout.flush()?;
                buffer.clear();
            }
        } else {
            buffer.push_str(&line);
            buffer.push('\n');
        }
    }

    Ok(())
}
