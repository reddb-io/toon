//! Calculator server using stdio transport
//!
//! Reads TOON-RPC request frames (spec §8.1) from stdin and writes response
//! frames to stdout. `calculator_stdio_client` spawns it.

use reddb_io_toon_rpc::{Dispatcher, Params};

fn extract_numbers(params: &Params) -> Result<Vec<f64>, reddb_io_toon_rpc::RpcError> {
    match params {
        Params::ByPosition(values) => values
            .iter()
            .map(|v| match v {
                serde_json::Value::Number(n) => n.as_f64().ok_or_else(|| {
                    reddb_io_toon_rpc::RpcError::InvalidParams("not a number".to_string())
                }),
                _ => Err(reddb_io_toon_rpc::RpcError::InvalidParams(
                    "expected numbers".to_string(),
                )),
            })
            .collect(),
        Params::ByName(_) => Err(reddb_io_toon_rpc::RpcError::InvalidParams(
            "named params not supported".to_string(),
        )),
        Params::Absent => Err(reddb_io_toon_rpc::RpcError::InvalidParams(
            "params are required".to_string(),
        )),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    eprintln!("Calculator stdio server ready");
    reddb_io_toon_rpc_stdio::serve_stdio(&dispatcher).await?;
    Ok(())
}
