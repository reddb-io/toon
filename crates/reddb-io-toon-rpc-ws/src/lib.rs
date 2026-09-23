//! TOON-RPC over WebSocket: one complete RPC document per message (§8).
//!
//! Text messages carry UTF-8 documents and binary messages carry raw ones; a
//! response goes back in the same kind of message as its request. A request
//! that produces no response (a notification) sends nothing back. Message and
//! frame sizes are capped explicitly.

use std::io;
use std::net::SocketAddr;

use futures::{SinkExt, StreamExt};
use reddb_io_toon_rpc::{
    dispatch_document, Dispatcher, DuplexTransport, RpcError, DEFAULT_MAX_FRAME_BYTES,
};
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// Largest message (and frame) accepted by default, in either direction.
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = DEFAULT_MAX_FRAME_BYTES;

fn config(max_message_bytes: usize) -> WebSocketConfig {
    WebSocketConfig {
        max_message_size: Some(max_message_bytes),
        max_frame_size: Some(max_message_bytes),
        ..WebSocketConfig::default()
    }
}

/// A WebSocket server bound to an address the caller chose.
pub struct WsServer {
    listener: TcpListener,
    dispatcher: Dispatcher,
    max_message_bytes: usize,
}

impl WsServer {
    pub async fn bind(addr: impl ToSocketAddrs, dispatcher: Dispatcher) -> io::Result<Self> {
        Ok(Self::from_listener(
            TcpListener::bind(addr).await?,
            dispatcher,
        ))
    }

    pub fn from_listener(listener: TcpListener, dispatcher: Dispatcher) -> Self {
        Self {
            listener,
            dispatcher,
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
        }
    }

    pub fn with_max_message_bytes(mut self, max_message_bytes: usize) -> Self {
        self.max_message_bytes = max_message_bytes;
        self
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Accept connections until accepting fails, serving each on its own task.
    pub async fn serve(self) -> io::Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let dispatcher = self.dispatcher.clone();
            let config = config(self.max_message_bytes);
            tokio::spawn(async move {
                if let Ok(ws) =
                    tokio_tungstenite::accept_async_with_config(stream, Some(config)).await
                {
                    serve_connection(ws, &dispatcher).await;
                }
            });
        }
    }
}

async fn serve_connection<S>(ws: tokio_tungstenite::WebSocketStream<S>, dispatcher: &Dispatcher)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();
    while let Some(message) = read.next().await {
        let reply = match message {
            Ok(WsMessage::Text(text)) => {
                let document = dispatch_document(dispatcher, text.as_bytes());
                // A TOON document is UTF-8, so this never needs the fallback.
                String::from_utf8(document)
                    .map(WsMessage::Text)
                    .unwrap_or_else(|error| WsMessage::Binary(error.into_bytes()))
            }
            Ok(WsMessage::Binary(data)) => WsMessage::Binary(dispatch_document(dispatcher, &data)),
            Ok(WsMessage::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        };
        let empty = match &reply {
            WsMessage::Text(text) => text.is_empty(),
            WsMessage::Binary(data) => data.is_empty(),
            _ => false,
        };
        if !empty && write.send(reply).await.is_err() {
            return;
        }
    }
    let _ = write.close().await;
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
        Self::connect_with_max_message_bytes(url, DEFAULT_MAX_MESSAGE_BYTES).await
    }

    pub async fn connect_with_max_message_bytes(
        url: &str,
        max_message_bytes: usize,
    ) -> Result<Self, RpcError> {
        let config = Some(config(max_message_bytes));
        let (ws, _) = tokio_tungstenite::connect_async_with_config(url, config, false)
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
    use reddb_io_toon_rpc::{Client, ClientOptions, Params};
    use serde_json::json;
    use std::time::Duration;

    async fn server(max_message_bytes: usize) -> String {
        let mut dispatcher = Dispatcher::new();
        dispatcher.register("echo", |params, _id| match params {
            Params::ByPosition(mut values) if !values.is_empty() => Ok(values.remove(0)),
            _ => Ok(json!(null)),
        });
        let server = WsServer::bind("127.0.0.1:0", dispatcher)
            .await
            .unwrap()
            .with_max_message_bytes(max_message_bytes);
        let url = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.serve());
        url
    }

    #[tokio::test]
    async fn concurrent_calls_are_correlated_by_id() {
        let client = Client::duplex(
            WsClient::connect(&server(DEFAULT_MAX_MESSAGE_BYTES).await)
                .await
                .unwrap(),
            ClientOptions::default(),
        );
        let calls = (0..8)
            .map(|n| tokio::spawn(client.call("echo", Params::ByPosition(vec![json!(n)]))))
            .collect::<Vec<_>>();
        for (n, call) in calls.into_iter().enumerate() {
            assert_eq!(call.await.unwrap().unwrap(), json!(n));
        }
        client.close().await.unwrap();
    }

    #[tokio::test]
    async fn a_notification_sends_nothing_back() {
        let transport = WsClient::connect(&server(DEFAULT_MAX_MESSAGE_BYTES).await)
            .await
            .unwrap();
        transport
            .send(b"toonrpc: \"1.0\"\nmethod: echo".to_vec())
            .await
            .unwrap();
        transport
            .send(b"toonrpc: \"1.0\"\nmethod: echo\nid: 1".to_vec())
            .await
            .unwrap();
        let first = transport.recv().await.unwrap().unwrap();
        let response = reddb_io_toon_rpc::response_from_wire(&first).unwrap();
        assert_eq!(response.id, reddb_io_toon_rpc::Id::Number(1));
    }

    #[tokio::test]
    async fn an_oversized_message_ends_the_connection() {
        let transport = WsClient::connect(&server(64).await).await.unwrap();
        transport.send(vec![b'a'; 1024]).await.unwrap();
        let ended = tokio::time::timeout(Duration::from_secs(5), transport.recv())
            .await
            .unwrap();
        assert!(!matches!(ended, Ok(Some(_))));
    }
}
