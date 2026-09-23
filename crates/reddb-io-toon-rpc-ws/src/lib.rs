use reddb_io_toon_rpc::{Dispatcher, DuplexTransport, RpcError};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMessage;

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

async fn handle_ws_connection<S>(ws: tokio_tungstenite::WebSocketStream<S>, dispatcher: Dispatcher)
where
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
                    Ok(bytes) => WsMessage::Binary(bytes),
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

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// WebSocket client transport: one RPC document per message.
pub struct WsClient {
    sink: tokio::sync::Mutex<futures::stream::SplitSink<WsStream, WsMessage>>,
    stream: tokio::sync::Mutex<futures::stream::SplitStream<WsStream>>,
}

impl WsClient {
    pub async fn connect(url: &str) -> Result<Self, RpcError> {
        use futures::StreamExt;
        let (ws, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))?;
        let (sink, stream) = ws.split();
        Ok(Self {
            sink: tokio::sync::Mutex::new(sink),
            stream: tokio::sync::Mutex::new(stream),
        })
    }
}

#[async_trait::async_trait]
impl DuplexTransport for WsClient {
    async fn send(&self, data: Vec<u8>) -> Result<(), RpcError> {
        use futures::SinkExt;
        let msg = match String::from_utf8(data) {
            Ok(s) => WsMessage::Text(s),
            Err(error) => WsMessage::Binary(error.into_bytes()),
        };
        self.sink
            .lock()
            .await
            .send(msg)
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))
    }

    async fn recv(&self) -> Result<Option<Vec<u8>>, RpcError> {
        use futures::StreamExt;
        let mut stream = self.stream.lock().await;
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(WsMessage::Text(s)) => return Ok(Some(s.into_bytes())),
                Ok(WsMessage::Binary(b)) => return Ok(Some(b)),
                Ok(WsMessage::Close(_)) => return Ok(None),
                Ok(_) => continue,
                Err(e) => return Err(RpcError::TransportError(e.to_string())),
            }
        }
        Ok(None)
    }

    async fn close(&self) -> Result<(), RpcError> {
        use futures::SinkExt;
        self.sink
            .lock()
            .await
            .close()
            .await
            .map_err(|e| RpcError::TransportError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ws_request_response() {
        let mut dispatcher = Dispatcher::new();
        dispatcher.register("echo", |_params, _id| Ok(serde_json::json!("hello back")));

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
        let client = reddb_io_toon_rpc::Client::duplex(
            WsClient::connect(&url).await.unwrap(),
            reddb_io_toon_rpc::ClientOptions::default(),
        );
        let result = client
            .call("echo", reddb_io_toon_rpc::Params::ByPosition(vec![]))
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!("hello back"));
        client.close().await.unwrap();
    }
}
