//! The calculator over HTTP.
//!
//! Usage: calculator_server [address]   (default 127.0.0.1:8080; port 0 picks one)
//! Prints `listening on http://<address>` once it is ready.

use reddb_io_toon_rpc_examples::calculator_dispatcher;
use reddb_io_toon_rpc_http::HttpServer;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".into());
    let server = HttpServer::bind(address, calculator_dispatcher()).await?;
    println!("listening on http://{}", server.local_addr()?);
    server.serve().await
}
