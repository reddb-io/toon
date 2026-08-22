//! Calculator client using HTTP transport
//!
//! Usage: cargo run --bin calculator_client <method> <a> <b>
//! Example: cargo run --bin calculator_client add 5 3

use reddb_io_toon_rpc::{
    from_wire, to_wire, ClientTransport, Id, Params, Request, RpcError, TOONRPC_VERSION,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Simple HTTP client using tokio directly
struct SimpleHttpClient {
    addr: SocketAddr,
    stream: Arc<Mutex<Option<TcpStream>>>,
}

impl SimpleHttpClient {
    async fn connect(addr: SocketAddr) -> Result<Self, RpcError> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        Ok(Self {
            addr,
            stream: Arc::new(Mutex::new(Some(stream))),
        })
    }
}

#[async_trait::async_trait]
impl ClientTransport for SimpleHttpClient {
    async fn send(&self, data: Vec<u8>) -> Result<(), RpcError> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("not connected".to_string()))?;

        let request = format!(
            "POST / HTTP/1.1\r\nHost: {}\r\nContent-Type: application/toon\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.addr, data.len()
        );

        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        stream
            .write_all(&data)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;

        Ok(())
    }

    async fn recv(&self) -> Result<Vec<u8>, RpcError> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("not connected".to_string()))?;

        let mut buffer = vec![0u8; 4096];
        let mut total = Vec::new();
        let mut header_end = None;
        let mut content_length = 0usize;
        let mut body_start = 0;

        loop {
            match stream.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    total.extend_from_slice(&buffer[..n]);

                    if header_end.is_none() {
                        if let Some(pos) = find_double_crlf(&total) {
                            header_end = Some(pos);
                            let headers = std::str::from_utf8(&total[..pos])
                                .map_err(|e| RpcError::TransportError(e.to_string()))?;
                            content_length = parse_content_length(headers);
                            body_start = pos + 4;
                        }
                    }

                    if let Some(end) = header_end {
                        if total.len() >= end + 4 + content_length {
                            break;
                        }
                    }
                }
                Err(e) => return Err(RpcError::TransportError(e.to_string())),
            }
        }

        let body = total[body_start..body_start + content_length].to_vec();
        Ok(body)
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
    let transport = SimpleHttpClient::connect(addr).await?;

    let request = Request {
        toonrpc: TOONRPC_VERSION.to_string(),
        method: method.clone(),
        params: Params::ByPosition(vec![serde_json::json!(a), serde_json::json!(b)]),
        id: Id::Number(1),
    };

    let msg = reddb_io_toon_rpc::protocol::Message::Single(
        reddb_io_toon_rpc::protocol::Call::Request(request),
    );
    let bytes = to_wire(&msg)?;
    transport.send(bytes).await?;

    let response = transport.recv().await?;
    let parsed = from_wire(&response)?;

    match parsed {
        reddb_io_toon_rpc::protocol::Message::SingleResponse(resp) => {
            match (resp.result, resp.error) {
                (Some(result), None) => {
                    println!("{} {} {} = {}", method, a, b, result);
                }
                (None, Some(error)) => {
                    eprintln!("Error: {}", error.message);
                }
                _ => eprintln!("Invalid response"),
            }
        }
        _ => eprintln!("Unexpected response type"),
    }

    Ok(())
}
