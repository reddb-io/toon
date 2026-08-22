use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use reddb_io_toon_rpc::{ClientTransport, Dispatcher, RpcError};

/// WebSocket server
pub struct WsServer {
    pub addr: SocketAddr,
    dispatcher: Dispatcher,
}

impl WsServer {
    pub fn new(addr: SocketAddr, dispatcher: Dispatcher) -> Self {
        Self { addr, dispatcher }
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;
        println!("TOON-RPC WebSocket server listening on ws://{}", self.addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let dispatcher = self.dispatcher.clone();
            tokio::spawn(async move {
                let ws_stream = tokio_tungstenite::accept_async(stream).await;
                match ws_stream {
                    Ok(ws) => {
                        handle_ws_connection(ws, dispatcher).await;
                    }
                    Err(e) => {
                        eprintln!("[WS] Error: {}", e);
                    }
                }
            });
        }
    }
}

async fn handle_ws_connection<S>(
    ws: tokio_tungstenite::WebSocketStream<S>,
    dispatcher: Dispatcher,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use futures::{SinkExt, StreamExt};
    let (mut write, mut read) = ws.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(WsMessage::Text(text)) => {
                let response = match dispatcher.dispatch(text.as_bytes()) {
                    Ok(bytes) => {
                        let s = String::from_utf8(bytes).unwrap_or_default();
                        WsMessage::Text(s)
                    }
                    Err(e) => {
                        let err = serde_json::json!({
                            "toonrpc": "1.0",
                            "error": {"code": -32603, "message": e.to_string()},
                            "id": null
                        });
                        WsMessage::Text(err.to_string())
                    }
                };
                if write.send(response).await.is_err() {
                    break;
                }
            }
            Ok(WsMessage::Binary(data)) => {
                let response = match dispatcher.dispatch(&data) {
                    Ok(bytes) => WsMessage::Binary(bytes.into()),
                    Err(e) => {
                        let err = serde_json::json!({
                            "toonrpc": "1.0",
                            "error": {"code": -32603, "message": e.to_string()},
                            "id": null
                        });
                        WsMessage::Text(err.to_string())
                    }
                };
                if write.send(response).await.is_err() {
                    break;
                }
            }
            Ok(WsMessage::Close(_)) => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
}

/// WebSocket client transport
pub struct WsClient {
    stream: tokio::sync::Mutex<
        Option<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
    >,
}

impl WsClient {
    pub async fn connect(url: &str) -> Result<Self, RpcError> {
        let (ws, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        Ok(Self {
            stream: tokio::sync::Mutex::new(Some(ws)),
        })
    }
}

#[async_trait::async_trait]
impl ClientTransport for WsClient {
    async fn send(&self, data: Vec<u8>) -> Result<(), RpcError> {
        use futures::SinkExt;
        let mut guard = self.stream.lock().await;
        let ws = guard
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("not connected".to_string()))?;

        let msg = match String::from_utf8(data.clone()) {
            Ok(s) => WsMessage::Text(s),
            Err(_) => WsMessage::Binary(data.into()),
        };
        ws.send(msg)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        Ok(())
    }

    async fn recv(&self) -> Result<Vec<u8>, RpcError> {
        use futures::StreamExt;
        let mut guard = self.stream.lock().await;
        let ws = guard
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("not connected".to_string()))?;

        while let Some(msg) = ws.next().await {
            match msg {
                Ok(WsMessage::Text(s)) => return Ok(s.into_bytes()),
                Ok(WsMessage::Binary(b)) => return Ok(b.to_vec()),
                Ok(_) => continue,
                Err(e) => return Err(RpcError::TransportError(e.to_string())),
            }
        }
        Err(RpcError::TransportError("connection closed".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ws_request_response() {
        let mut dispatcher = Dispatcher::new();
        dispatcher.register("echo", |_params, _id| {
            Ok(serde_json::json!("hello back"))
        });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let dispatcher_clone = dispatcher.clone();
        tokio::spawn(async move {
            let server = WsServer::new(addr, dispatcher_clone);
            let _ = server.serve().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let url = format!("ws://{}", addr);
        let client = WsClient::connect(&url).await.unwrap();

        let request = reddb_io_toon_rpc::protocol::Message::Single(
            reddb_io_toon_rpc::protocol::Call::Request(reddb_io_toon_rpc::protocol::Request::new(
                "echo".to_string(),
                reddb_io_toon_rpc::types::Params::ByPosition(vec![]),
                reddb_io_toon_rpc::types::Id::Number(1),
            ))
        );
        let bytes = reddb_io_toon_rpc::to_wire(&request).unwrap();
        client.send(bytes).await.unwrap();
        let response = client.recv().await.unwrap();

        let response_msg = reddb_io_toon_rpc::from_wire(&response).unwrap();
        match response_msg {
            reddb_io_toon_rpc::protocol::Message::SingleResponse(resp) => {
                assert!(resp.result.is_some());
            }
            _ => panic!("Expected SingleResponse"),
        }
    }
}
