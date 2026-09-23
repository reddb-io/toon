//! Call the HTTP calculator through the generated client.
//!
//! Usage: calculator_client <add|divide> <a> <b> [url]   (default http://127.0.0.1:8080/)

use reddb_io_toon_rpc::{Client, ClientOptions};
use reddb_io_toon_rpc_examples::calculator_api::CalculatorClient;
use reddb_io_toon_rpc_http::HttpTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 4 {
        eprintln!("usage: {} <add|divide> <a> <b> [url]", args[0]);
        std::process::exit(2);
    }
    let (method, a, b) = (&args[1], args[2].parse::<f64>()?, args[3].parse::<f64>()?);
    let url = args.get(4).map_or("http://127.0.0.1:8080/", String::as_str);
    let transport = HttpTransport::new(url.parse()?);
    let calculator = CalculatorClient::new(Client::request_response(
        transport,
        ClientOptions::default(),
    ));
    let result = match method.as_str() {
        "add" => calculator.add(a, b).await,
        "divide" => calculator.divide(a, b).await,
        other => {
            eprintln!("unknown method {other}");
            std::process::exit(2);
        }
    };
    match result {
        Ok(value) => println!("{method} {a} {b} = {value}"),
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
    Ok(())
}
