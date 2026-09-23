//! The MCP stdio transport: newline-delimited JSON-RPC messages, one per
//! line with no embedded newlines. Anything else the server prints must go to
//! stderr.

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::{McpServer, McpService};

/// Serve one MCP session on this process's stdin and stdout until stdin ends.
pub async fn serve_stdio<S: McpService>(server: &McpServer<S>) -> std::io::Result<()> {
    serve_lines(
        server,
        tokio::io::stdin(),
        tokio::io::stdout(),
        16 * 1024 * 1024,
    )
    .await
}

/// Serve one MCP session over any line stream. A line longer than
/// `max_line_bytes` ends the session with a JSON-RPC error.
pub async fn serve_lines<S, R, W>(
    server: &McpServer<S>,
    input: R,
    mut output: W,
    max_line_bytes: usize,
) -> std::io::Result<()>
where
    S: McpService,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut input = BufReader::new(input);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = (&mut input)
            .take(max_line_bytes as u64 + 1)
            .read_until(b'\n', &mut line)
            .await?;
        if read == 0 {
            break;
        }
        if line.len() > max_line_bytes && line.last() != Some(&b'\n') {
            let refusal = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Message exceeds the size limit"}}"#;
            output.write_all(refusal.as_bytes()).await?;
            output.write_all(b"\n").await?;
            break;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\n', '\r']);
        if text.trim().is_empty() {
            continue;
        }
        if let Some(answer) = server.handle_line(text) {
            output.write_all(answer.as_bytes()).await?;
            output.write_all(b"\n").await?;
            output.flush().await?;
        }
    }
    output.shutdown().await
}
