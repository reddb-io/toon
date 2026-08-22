use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use toon_rpc::{ClientTransport, Dispatcher, RpcError};

/// TCP server that handles TOON-RPC requests line-by-line (newline-delimited)
pub struct TcpServer {
    pub addr: SocketAddr,
    dispatcher: Dispatcher,
}

impl TcpServer {
    pub fn new(addr: SocketAddr, dispatcher: Dispatcher) -> Self {
        Self { addr, dispatcher }
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;
        println!("TOON-RPC TCP server listening on {}", self.addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let dispatcher = self.dispatcher.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, dispatcher).await {
                    eprintln!("[TCP] Connection error: {}", e);
                }
            });
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    dispatcher: Dispatcher,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut buffer = String::new();

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // EOF
            if !buffer.is_empty() {
                let response = dispatch_to_string(&dispatcher, &buffer);
                write_half.write_all(response.as_bytes()).await?;
            }
            break;
        }

        // Empty line marks end of a TOON message
        if line == "\n" || line == "\r\n" {
            if !buffer.is_empty() {
                let response = dispatch_to_string(&dispatcher, &buffer);
                write_half.write_all(response.as_bytes()).await?;
                buffer.clear();
            }
        } else {
            buffer.push_str(&line);
        }
    }

    Ok(())
}

fn dispatch_to_string(dispatcher: &Dispatcher, buffer: &str) -> String {
    match dispatcher.dispatch(buffer.trim().as_bytes()) {
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
    }
}

/// TCP client transport
pub struct TcpClient {
    pub addr: SocketAddr,
    stream: Mutex<Option<TcpStream>>,
}

impl TcpClient {
    pub async fn connect(addr: SocketAddr) -> Result<Self, RpcError> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        Ok(Self {
            addr,
            stream: Mutex::new(Some(stream)),
        })
    }
}

#[async_trait::async_trait]
impl ClientTransport for TcpClient {
    async fn send(&self, data: Vec<u8>) -> Result<(), RpcError> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("not connected".to_string()))?;
        let mut payload = data;
        payload.push(b'\n');
        payload.push(b'\n');
        stream
            .write_all(&payload)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        Ok(())
    }

    async fn recv(&self) -> Result<Vec<u8>, RpcError> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("not connected".to_string()))?;
        use tokio::io::AsyncBufReadExt;
        let mut reader = BufReader::new(stream);
        let mut buffer = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await
                .map_err(|e| RpcError::TransportError(e.to_string()))?;
            if n == 0 {
                break;
            }
            if line == "\n" || line == "\r\n" {
                break;
            }
            buffer.push_str(&line);
        }
        Ok(buffer.trim().as_bytes().to_vec())
    }
}

/// Unix Socket server
#[cfg(unix)]
pub struct UnixServer {
    pub path: std::path::PathBuf,
    dispatcher: Dispatcher,
}

#[cfg(unix)]
impl UnixServer {
    pub fn new(path: impl Into<std::path::PathBuf>, dispatcher: Dispatcher) -> Self {
        Self {
            path: path.into(),
            dispatcher,
        }
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::net::UnixListener;
        // Remove existing socket file if present
        let _ = std::fs::remove_file(&self.path);
        let listener = UnixListener::bind(&self.path)?;
        println!("TOON-RPC Unix Socket server listening on {:?}", self.path);

        loop {
            let (stream, _) = listener.accept().await?;
            let dispatcher = self.dispatcher.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_unix_connection(stream, dispatcher).await {
                    eprintln!("[Unix] Connection error: {}", e);
                }
            });
        }
    }
}

#[cfg(unix)]
async fn handle_unix_connection(
    stream: tokio::net::UnixStream,
    dispatcher: Dispatcher,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut buffer = String::new();

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            if !buffer.is_empty() {
                let response = dispatch_to_string(&dispatcher, &buffer);
                write_half.write_all(response.as_bytes()).await?;
            }
            break;
        }

        if line == "\n" || line == "\r\n" {
            if !buffer.is_empty() {
                let response = dispatch_to_string(&dispatcher, &buffer);
                write_half.write_all(response.as_bytes()).await?;
                buffer.clear();
            }
        } else {
            buffer.push_str(&line);
        }
    }

    Ok(())
}

/// Unix Socket client
#[cfg(unix)]
pub struct UnixClient {
    pub path: std::path::PathBuf,
    stream: Mutex<Option<tokio::net::UnixStream>>,
}

#[cfg(unix)]
impl UnixClient {
    pub async fn connect(path: impl Into<std::path::PathBuf>) -> Result<Self, RpcError> {
        let path = path.into();
        let stream = tokio::net::UnixStream::connect(&path)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        Ok(Self {
            path,
            stream: Mutex::new(Some(stream)),
        })
    }
}

#[cfg(unix)]
#[async_trait::async_trait]
impl ClientTransport for UnixClient {
    async fn send(&self, data: Vec<u8>) -> Result<(), RpcError> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("not connected".to_string()))?;
        let mut payload = data;
        payload.push(b'\n');
        payload.push(b'\n');
        stream
            .write_all(&payload)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        Ok(())
    }

    async fn recv(&self) -> Result<Vec<u8>, RpcError> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("not connected".to_string()))?;
        use tokio::io::AsyncBufReadExt;
        let mut reader = BufReader::new(stream);
        let mut buffer = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await
                .map_err(|e| RpcError::TransportError(e.to_string()))?;
            if n == 0 {
                break;
            }
            if line == "\n" || line == "\r\n" {
                break;
            }
            buffer.push_str(&line);
        }
        Ok(buffer.trim().as_bytes().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_request_response() {
        let mut dispatcher = Dispatcher::new();
        dispatcher.register("echo", |_params, _id| {
            Ok(serde_json::json!("hello back"))
        });

        // Bind to a random port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        // Start server
        let dispatcher_clone = dispatcher.clone();
        tokio::spawn(async move {
            let server = TcpServer::new(addr, dispatcher_clone);
            let _ = server.serve().await;
        });

        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Connect client and send a request
        let client = TcpClient::connect(addr).await.unwrap();

        // Build a proper TOON-RPC request using to_wire
        let request_msg = toon_rpc::protocol::Message::Single(
            toon_rpc::protocol::Call::Request(toon_rpc::protocol::Request::new(
                "echo".to_string(),
                toon_rpc::types::Params::ByPosition(vec![serde_json::Value::String("hello".to_string())]),
                toon_rpc::types::Id::Number(1),
            ))
        );
        let bytes = toon_rpc::to_wire(&request_msg).unwrap();
        eprintln!("Request TOON: {}", String::from_utf8_lossy(&bytes));
        client.send(bytes).await.unwrap();
        let response = client.recv().await.unwrap();
        eprintln!("Response: {}", String::from_utf8_lossy(&response));

        let response_msg = toon_rpc::from_wire(&response).unwrap();
        match response_msg {
            toon_rpc::protocol::Message::SingleResponse(resp) => {
                assert!(resp.result.is_some());
            }
            _ => panic!("Expected SingleResponse"),
        }
    }
}
