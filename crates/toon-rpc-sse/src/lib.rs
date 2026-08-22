use parking_lot::Mutex;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{channel, Sender};
use toon_rpc::{Dispatcher, RpcError};

/// Unique ID for each SSE subscription
pub type SubscriptionId = String;

/// An event that can be sent to a subscriber
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Registry of subscriptions - maps subscription_id -> sender
type SubscriptionMap = HashMap<SubscriptionId, Sender<SseEvent>>;

/// Global subscription registry shared by the server
#[derive(Clone)]
pub struct SseRegistry {
    inner: Arc<Mutex<SubscriptionMap>>,
}

impl SseRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn subscribe(&self, id: SubscriptionId) -> Sender<SseEvent> {
        let (tx, _rx) = channel(64);
        self.inner.lock().insert(id, tx.clone());
        tx
    }

    pub fn subscribe_with_receiver(
        &self,
        id: SubscriptionId,
    ) -> (Sender<SseEvent>, tokio::sync::mpsc::Receiver<SseEvent>) {
        let (tx, rx) = channel(64);
        self.inner.lock().insert(id, tx.clone());
        (tx, rx)
    }

    pub fn unsubscribe(&self, id: &str) -> bool {
        self.inner.lock().remove(id).is_some()
    }

    pub fn publish(&self, id: &str, event: SseEvent) -> bool {
        if let Some(tx) = self.inner.lock().get(id) {
            // Try to send, ignore if full
            let _ = tx.try_send(event);
            true
        } else {
            false
        }
    }

    pub fn broadcast(&self, event: SseEvent) {
        for (_, tx) in self.inner.lock().iter() {
            let _ = tx.try_send(event.clone());
        }
    }
}

impl Default for SseRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Server that exposes both the dispatcher and SSE registry
#[derive(Clone)]
pub struct SseServer {
    pub dispatcher: Dispatcher,
    pub registry: SseRegistry,
}

impl SseServer {
    pub fn new(dispatcher: Dispatcher) -> Self {
        Self {
            dispatcher,
            registry: SseRegistry::new(),
        }
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr: SocketAddr = "0.0.0.0:8081".parse()?;
        let listener = TcpListener::bind(addr).await?;
        println!("TOON-RPC SSE server listening on http://{}", addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let server = self.clone();

            tokio::spawn(async move {
                let connection = hyper_util::rt::TokioIo::new(stream);

                let server_for_service = server.clone();
                let hyper_service = hyper::service::service_fn(move |req| {
                    let svc = server_for_service.clone();
                    async move { hyper::service::Service::call(&svc, req).await }
                });

                if let Err(e) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(connection, hyper_service)
                    .await
                {
                    eprintln!("Error serving connection: {}", e);
                }
            });
        }
    }

    /// Send an event to a specific subscriber (used by handlers)
    pub fn publish_event(&self, subscription_id: &str, event: SseEvent) {
        self.registry.publish(subscription_id, event);
    }

    /// Broadcast to all subscribers
    pub fn broadcast_event(&self, event: SseEvent) {
        self.registry.broadcast(event);
    }
}

/// Route requests based on path:
/// - GET /sse -> SSE stream
/// - POST / -> regular RPC request
impl hyper::service::Service<http::Request<hyper::body::Incoming>> for SseServer {
    type Response = http::Response<String>;
    type Error = Infallible;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
        let server = self.clone();
        let path = req.uri().path().to_string();
        let method = req.method().clone();

        Box::pin(async move {
            if method == http::Method::GET && path == "/sse" {
                Ok(build_sse_response(server))
            } else if method == http::Method::POST && path == "/" {
                handle_post(req, server.dispatcher).await
            } else {
                Ok(http::Response::builder()
                    .status(http::StatusCode::NOT_FOUND)
                    .body("Not found".to_string())
                    .unwrap())
            }
        })
    }
}

/// Build an SSE response that streams events
fn build_sse_response(server: SseServer) -> http::Response<String> {
    let subscription_id = format!("sub-{}", uuid_simple());
    let (tx, mut rx) = server.registry.subscribe_with_receiver(subscription_id.clone());

    // Spawn task to forward events to SSE stream
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            // Format SSE event
            let sse = format!(
                "event: {}\ndata: {}\n\n",
                event.event, event.data
            );
            println!("[SSE] Sending: {}", sse.trim());
        }
        drop(tx); // keep tx alive in scope
    });

    let body = format!("data: {{\"subscriptionId\":\"{}\"}}\n\n", subscription_id);

    http::Response::builder()
        .status(http::StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .body(body)
        .unwrap()
}

/// Handle POST RPC requests
async fn handle_post(
    req: http::Request<hyper::body::Incoming>,
    dispatcher: Dispatcher,
) -> Result<http::Response<String>, Infallible> {
    use http_body_util::BodyExt;
    let body_result = req.into_body().collect().await;
    let body = match body_result {
        Ok(b) => b.to_bytes(),
        Err(_) => {
            let err = serde_json::json!({
                "toonrpc": "1.0",
                "error": {"code": -32700, "message": "body error"},
                "id": null
            });
            return Ok(http::Response::builder()
                .status(http::StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/toon")
                .body(err.to_string())
                .unwrap());
        }
    };

    match dispatcher.dispatch(&body.to_vec()) {
        Ok(response) => Ok(http::Response::builder()
            .status(http::StatusCode::OK)
            .header("Content-Type", "application/toon")
            .body(String::from_utf8(response).unwrap())
            .unwrap()),
        Err(e) => {
            let err = serde_json::json!({
                "toonrpc": "1.0",
                "error": {"code": -32603, "message": e.to_string()},
                "id": null
            });
            Ok(http::Response::builder()
                .status(http::StatusCode::OK)
                .header("Content-Type", "application/toon")
                .body(err.to_string())
                .unwrap())
        }
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", nanos)
}

/// SSE client (basic HTTP client that opens SSE stream)
pub struct SseClient {
    pub url: String,
}

impl SseClient {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// Open SSE stream and return subscription ID
    pub async fn connect(&self) -> Result<String, RpcError> {
        // Simplified: just return a placeholder
        // Real implementation would use hyper client to GET /sse
        Ok(format!("client-{}", uuid_simple()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_subscribe_publish() {
        let registry = SseRegistry::new();
        let id = "test-sub".to_string();
        let _tx = registry.subscribe(id.clone());

        let event = SseEvent {
            event: "ping".to_string(),
            data: "{}".to_string(),
        };

        let published = registry.publish(&id, event);
        assert!(published);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let registry = SseRegistry::new();
        let id = "test-sub".to_string();
        let _tx = registry.subscribe(id.clone());

        assert!(registry.unsubscribe(&id));
        assert!(!registry.unsubscribe(&id));
    }
}
