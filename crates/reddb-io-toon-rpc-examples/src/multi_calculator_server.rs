//! Multi-protocol calculator server
//!
//! Listens on stdin, accepts requests in **either** JSON-RPC 2.0 or TOON-RPC 1.0,
//! and responds in the same format the client used.
//!
//! Test from a shell:
//!
//! ```bash
//! # JSON-RPC request → JSON-RPC response
//! echo '{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}' \
//!   | cargo run --bin multi_calculator_server
//!
//! # TOON-RPC request → TOON-RPC response
//! printf 'toonrpc: "1.0"\nmethod: add\nparams[2]: 2,3\nid: 1\n\n' \
//!   | cargo run --bin multi_calculator_server
//! ```

use reddb_io_toon_rpc::multi::MultiRpc;
use reddb_io_toon_rpc::{Dispatcher, Params, RpcError};
use std::io::{self, BufRead, Write};

fn build_dispatcher() -> Dispatcher {
    let mut d = Dispatcher::new();
    d.register("add", |params, _id| {
        let nums = match params {
            Params::ByPosition(arr) => arr,
            _ => return Err(RpcError::InvalidParams("expected array".into())),
        };
        let a = nums[0]
            .as_i64()
            .ok_or_else(|| RpcError::InvalidParams("a".into()))?;
        let b = nums[1]
            .as_i64()
            .ok_or_else(|| RpcError::InvalidParams("b".into()))?;
        Ok(serde_json::json!(a + b))
    });
    d.register("subtract", |params, _id| {
        let nums = match params {
            Params::ByPosition(arr) => arr,
            _ => return Err(RpcError::InvalidParams("expected array".into())),
        };
        let a = nums[0]
            .as_i64()
            .ok_or_else(|| RpcError::InvalidParams("a".into()))?;
        let b = nums[1]
            .as_i64()
            .ok_or_else(|| RpcError::InvalidParams("b".into()))?;
        Ok(serde_json::json!(a - b))
    });
    d.register("multiply", |params, _id| {
        let nums = match params {
            Params::ByPosition(arr) => arr,
            _ => return Err(RpcError::InvalidParams("expected array".into())),
        };
        let a = nums[0]
            .as_i64()
            .ok_or_else(|| RpcError::InvalidParams("a".into()))?;
        let b = nums[1]
            .as_i64()
            .ok_or_else(|| RpcError::InvalidParams("b".into()))?;
        Ok(serde_json::json!(a * b))
    });
    d
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dispatcher = build_dispatcher();
    let multi = MultiRpc::new(dispatcher);

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    writeln!(stderr, "[multi-rpc] server ready — accepts JSON-RPC 2.0 and TOON-RPC 1.0")?;
    stderr.flush()?;

    let mut buffer = String::new();
    for line in stdin.lock().lines() {
        let line = line?;

        // Two framing conventions:
        //   - TOON-RPC: messages end with an empty line
        //   - JSON-RPC: a single-line object with balanced braces fires immediately
        buffer.push_str(&line);
        buffer.push('\n');

        // Heuristic: JSON-RPC single-line object `{...}` is a complete request.
        // Multi-line JSON or TOON waits for the empty-line terminator.
        let is_single_line_json = {
            let trimmed = buffer.trim();
            trimmed.starts_with('{')
                && trimmed.ends_with('}')
                && !trimmed.contains('\n')
        };

        if line.is_empty() || is_single_line_json {
            let request = std::mem::take(&mut buffer);
            let (protocol, response) = multi
                .handle_with_protocol(request.as_bytes(), None)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                })?;

            let protocol_name = match protocol {
                reddb_io_toon_rpc::multi::Protocol::JsonRpc => "jsonrpc",
                reddb_io_toon_rpc::multi::Protocol::ToonRpc => "toonrpc",
            };
            writeln!(
                stderr,
                "[multi-rpc] detected: {}, response {} bytes",
                protocol_name,
                response.len()
            )?;

            if !response.is_empty() {
                stdout.write_all(&response)?;
                stdout.write_all(b"\n\n")?;
                stdout.flush()?;
            }
        }
    }
    Ok(())
}
