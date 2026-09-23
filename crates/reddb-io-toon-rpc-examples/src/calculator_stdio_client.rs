//! Calculator client using the stdio transport
//!
//! Spawns `calculator_stdio_server` (built next to this binary) and calls it
//! over the child's pipes with §8.1 framing.
//!
//! Usage: cargo run --bin calculator_stdio_client <method> <a> <b>
//! Example: cargo run --bin calculator_stdio_client add 5 3

use reddb_io_toon_rpc::{Client, ClientOptions, Params};
use tokio::process::Command;

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

    let server = std::env::current_exe()?.with_file_name("calculator_stdio_server");
    let (transport, _child) = reddb_io_toon_rpc_stdio::spawn(&mut Command::new(server))?;
    let client = Client::duplex(transport, ClientOptions::default());
    let params = Params::ByPosition(vec![serde_json::json!(a), serde_json::json!(b)]);
    match client.call(method, params).await {
        Ok(result) => println!("{method} {a} {b} = {result}"),
        Err(error) => eprintln!("Error: {error}"),
    }
    client.close().await?;
    Ok(())
}
