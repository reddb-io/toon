//! Spawn `calculator_stdio_server` (built next to this binary) and call it
//! through the generated client.
//!
//! Usage: calculator_stdio_client <add|divide> <a> <b>

use reddb_io_toon_rpc::{Client, ClientOptions};
use reddb_io_toon_rpc_examples::calculator_api::CalculatorClient;
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        eprintln!("usage: {} <add|divide> <a> <b>", args[0]);
        std::process::exit(2);
    }
    let (method, a, b) = (&args[1], args[2].parse::<f64>()?, args[3].parse::<f64>()?);
    let server = std::env::current_exe()?.with_file_name("calculator_stdio_server");
    let (transport, _child) = reddb_io_toon_rpc_stdio::spawn(&mut Command::new(server))?;
    let client = Client::duplex(transport, ClientOptions::default());
    let calculator = CalculatorClient::new(client.clone());
    let result = match method.as_str() {
        "add" => calculator.add(a, b).await,
        "divide" => calculator.divide(a, b).await,
        other => {
            eprintln!("unknown method {other}");
            std::process::exit(2);
        }
    };
    client.close().await?;
    match result {
        Ok(value) => println!("{method} {a} {b} = {value}"),
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
    Ok(())
}
