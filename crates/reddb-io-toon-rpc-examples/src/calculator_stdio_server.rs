//! Calculator server using stdio transport
//!
//! Reads TOON-RPC requests from stdin, writes responses to stdout
//! Messages are delimited by empty lines (\n\n)
//!
//! Run with: cargo run --bin calculator_stdio_server

use std::io::{self, BufRead, Write};
use reddb_io_toon_rpc::{Dispatcher, Params};

fn extract_numbers(params: &Params) -> Result<Vec<f64>, reddb_io_toon_rpc::RpcError> {
    match params {
        Params::ByPosition(values) => values
            .iter()
            .map(|v| match v {
                serde_json::Value::Number(n) => n
                    .as_f64()
                    .ok_or_else(|| reddb_io_toon_rpc::RpcError::InvalidParams("not a number".to_string())),
                _ => Err(reddb_io_toon_rpc::RpcError::InvalidParams("expected numbers".to_string())),
            })
            .collect(),
        Params::ByName(_) => Err(reddb_io_toon_rpc::RpcError::InvalidParams(
            "named params not supported".to_string(),
        )),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut dispatcher = Dispatcher::new();

    dispatcher.register("add", |params, _id| {
        let nums = extract_numbers(&params)?;
        Ok(serde_json::json!(nums[0] + nums[1]))
    });
    dispatcher.register("subtract", |params, _id| {
        let nums = extract_numbers(&params)?;
        Ok(serde_json::json!(nums[0] - nums[1]))
    });
    dispatcher.register("multiply", |params, _id| {
        let nums = extract_numbers(&params)?;
        Ok(serde_json::json!(nums[0] * nums[1]))
    });
    dispatcher.register("divide", |params, _id| {
        let nums = extract_numbers(&params)?;
        if nums[1] == 0.0 {
            Err(reddb_io_toon_rpc::RpcError::InvalidParams(
                "division by zero".to_string(),
            ))
        } else {
            Ok(serde_json::json!(nums[0] / nums[1]))
        }
    });

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    eprintln!("Calculator stdio server ready");

    let mut buffer = String::new();
    for line in stdin.lock().lines() {
        let line = line?;

        // Empty line marks end of a TOON message
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
                            "toonrpc": "1.0",
                            "error": {"code": -32603, "message": e.to_string()},
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
