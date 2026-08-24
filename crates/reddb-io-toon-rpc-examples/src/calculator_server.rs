//! Calculator server using HTTP transport
//!
//! Run with: cargo run --bin calculator_server

use reddb_io_toon_rpc::{Dispatcher, Params};
use reddb_io_toon_rpc_http::HttpService;
use std::net::SocketAddr;

fn extract_numbers(params: &Params) -> Result<Vec<f64>, reddb_io_toon_rpc::RpcError> {
    match params {
        Params::ByPosition(values) => {
            let mut nums = Vec::new();
            for v in values {
                match v {
                    serde_json::Value::Number(n) => {
                        nums.push(n.as_f64().ok_or_else(|| {
                            reddb_io_toon_rpc::RpcError::InvalidParams("not a number".to_string())
                        })?);
                    }
                    _ => {
                        return Err(reddb_io_toon_rpc::RpcError::InvalidParams(
                            "expected numbers".to_string(),
                        ))
                    }
                }
            }
            Ok(nums)
        }
        Params::ByName(_) => Err(reddb_io_toon_rpc::RpcError::InvalidParams(
            "named params not supported".to_string(),
        )),
        Params::Absent => Err(reddb_io_toon_rpc::RpcError::InvalidParams(
            "params are required".to_string(),
        )),
    }
}

fn build_dispatcher() -> Dispatcher {
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

    dispatcher
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dispatcher = build_dispatcher();
    let addr: SocketAddr = "127.0.0.1:8080".parse()?;
    let service = HttpService::new(dispatcher);
    println!("Calculator HTTP server listening on http://{}", addr);
    service.serve().await
}
