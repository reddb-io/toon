//! Calculator client using HTTP transport
//!
//! Usage: cargo run --bin calculator_client <method> <a> <b>
//! Example: cargo run --bin calculator_client add 5 3

use reddb_io_toon_rpc::{Client, ClientOptions, Params};
use reddb_io_toon_rpc_http::HttpTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: {} <method> <a> <b>", args[0]);
        std::process::exit(1);
    }

    let method = &args[1];
    let a: f64 = args[2].parse()?;
    let b: f64 = args[3].parse()?;

    let transport = HttpTransport::new("http://127.0.0.1:8080/".parse()?);
    let client = Client::request_response(transport, ClientOptions::default());
    let params = Params::ByPosition(vec![serde_json::json!(a), serde_json::json!(b)]);
    match client.call(method, params).await {
        Ok(result) => println!("{} {} {} = {}", method, a, b, result),
        Err(error) => eprintln!("Error: {}", error),
    }

    Ok(())
}
