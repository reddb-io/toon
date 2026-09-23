//! Calculator client using HTTP transport
//!
//! Usage: cargo run --bin calculator_client <method> <a> <b>
//! Example: cargo run --bin calculator_client add 5 3

use reddb_io_toon_rpc::{Client, ClientOptions, Params, RequestResponseTransport, RpcError};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Minimal HTTP/1.1 request/response transport: one connection per request.
struct SimpleHttpClient {
    addr: SocketAddr,
}

#[async_trait::async_trait]
impl RequestResponseTransport for SimpleHttpClient {
    async fn request(&self, data: Vec<u8>) -> Result<Option<Vec<u8>>, RpcError> {
        let transport = |e: std::io::Error| RpcError::TransportError(e.to_string());
        let mut stream = TcpStream::connect(self.addr).await.map_err(transport)?;
        let head = format!(
            "POST / HTTP/1.1\r\nHost: {}\r\nContent-Type: application/toon\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.addr,
            data.len()
        );
        stream.write_all(head.as_bytes()).await.map_err(transport)?;
        stream.write_all(&data).await.map_err(transport)?;

        let mut total = Vec::new();
        stream.read_to_end(&mut total).await.map_err(transport)?;
        let Some(end) = find_double_crlf(&total) else {
            return Err(RpcError::TransportError("malformed HTTP response".into()));
        };
        let headers = std::str::from_utf8(&total[..end])
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        let length = parse_content_length(headers).min(total.len() - end - 4);
        Ok(Some(total[end + 4..end + 4 + length].to_vec()))
    }
}

fn find_double_crlf(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(3) {
        if &data[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

fn parse_content_length(headers: &str) -> usize {
    for line in headers.lines() {
        if let Some(rest) = line.to_lowercase().strip_prefix("content-length:") {
            return rest.trim().parse().unwrap_or(0);
        }
    }
    0
}

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

    let addr: SocketAddr = "127.0.0.1:8080".parse()?;
    let client = Client::request_response(SimpleHttpClient { addr }, ClientOptions::default());
    let params = Params::ByPosition(vec![serde_json::json!(a), serde_json::json!(b)]);
    match client.call(method, params).await {
        Ok(result) => println!("{} {} {} = {}", method, a, b, result),
        Err(error) => eprintln!("Error: {}", error),
    }

    Ok(())
}
