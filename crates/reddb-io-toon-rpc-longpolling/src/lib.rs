use http_body_util::BodyExt;
use http::{Request, Response, StatusCode};
use hyper::body::Incoming;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use reddb_io_toon_rpc::Dispatcher;

/// Maps poll_id -> waiting channels
type PendingPolls = Arc<Mutex<HashMap<String, Vec<oneshot::Sender<String>>>>>;

#[derive(Clone)]
pub struct LongPollingServer {
    dispatcher: Dispatcher,
    pending: PendingPolls,
}

impl LongPollingServer {
    pub fn new(dispatcher: Dispatcher) -> Self {
        Self {
            dispatcher,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Send a notification to all waiters of a poll_id
    pub fn push_event(&self, poll_id: &str, data: String) {
        let mut pending = self.pending.lock();
        if let Some(waiters) = pending.get_mut(poll_id) {
            let drained: Vec<_> = waiters.drain(..).collect();
            for waiter in drained {
                let _ = waiter.send(data.clone());
            }
        }
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr: SocketAddr = "0.0.0.0:8082".parse()?;
        let listener = TcpListener::bind(addr).await?;
        println!("TOON-RPC Long Polling server listening on http://{}", addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let service = self.clone();

            tokio::spawn(async move {
                let connection = hyper_util::rt::TokioIo::new(stream);

                let hyper_service = hyper::service::service_fn(move |req| {
                    let svc = service.clone();
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
}

impl hyper::service::Service<Request<Incoming>> for LongPollingServer {
    type Response = Response<String>;
    type Error = Infallible;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let dispatcher = self.dispatcher.clone();
        let pending = self.pending.clone();
        let path = req.uri().path().to_string();
        let method = req.method().clone();

        Box::pin(async move {
            if method == http::Method::POST && path == "/rpc" {
                handle_rpc(req, dispatcher).await
            } else if method == http::Method::GET && path.starts_with("/poll/") {
                let poll_id = path.trim_start_matches("/poll/").to_string();
                handle_poll(poll_id, pending).await
            } else if method == http::Method::POST && path.starts_with("/notify/") {
                let poll_id = path.trim_start_matches("/notify/").to_string();
                handle_notify(req, poll_id, pending).await
            } else {
                Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body("Not found".to_string())
                    .unwrap())
            }
        })
    }
}

/// Handle regular RPC POST request
async fn handle_rpc(
    req: Request<Incoming>,
    dispatcher: Dispatcher,
) -> Result<Response<String>, Infallible> {
    let body = match req.into_body().collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => {
            let err = serde_json::json!({
                "toonrpc": "1.0",
                "error": {"code": -32700, "message": "body error"},
                "id": null
            });
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/toon")
                .body(err.to_string())
                .unwrap());
        }
    };

    match dispatcher.dispatch(&body.to_vec()) {
        Ok(bytes) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/toon")
            .body(String::from_utf8(bytes).unwrap())
            .unwrap()),
        Err(e) => {
            let err = serde_json::json!({
                "toonrpc": "1.0",
                "error": {"code": -32603, "message": e.to_string()},
                "id": null
            });
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/toon")
                .body(err.to_string())
                .unwrap())
        }
    }
}

/// Long-poll endpoint: holds request open until event arrives or timeout
async fn handle_poll(
    poll_id: String,
    pending: PendingPolls,
) -> Result<Response<String>, Infallible> {
    let (tx, rx) = oneshot::channel::<String>();

    // Register waiter
    {
        let mut pending = pending.lock();
        pending.entry(poll_id.clone()).or_default().push(tx);
    }

    // Wait with timeout (30 seconds)
    let result = tokio::time::timeout(Duration::from_secs(30), rx).await;

    match result {
        Ok(Ok(data)) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/toon")
            .body(data)
            .unwrap()),
        Ok(Err(_)) => Ok(Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body("".to_string())
            .unwrap()),
        Err(_) => {
            // Timeout: remove from pending
            let mut pending = pending.lock();
            if let Some(waiters) = pending.get_mut(&poll_id) {
                waiters.retain(|w| !w.is_closed());
            }
            Ok(Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body("".to_string())
                .unwrap())
        }
    }
}

/// Notify endpoint: push event to all waiters of poll_id
async fn handle_notify(
    req: Request<Incoming>,
    poll_id: String,
    pending: PendingPolls,
) -> Result<Response<String>, Infallible> {
    // Drain waiters and send data
    let body = match req.into_body().collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("body error".to_string())
                .unwrap());
        }
    };

    let data = String::from_utf8_lossy(&body).to_string();

    let waiters: Vec<_> = {
        let mut pending = pending.lock();
        if let Some(waiters) = pending.get_mut(&poll_id) {
            waiters.drain(..).collect()
        } else {
            vec![]
        }
    };

    let count = waiters.len();
    for waiter in waiters {
        let _ = waiter.send(data.clone());
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(format!("notified {} waiters", count))
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_long_poll_event_push() {
        let dispatcher = Dispatcher::new();
        let server = LongPollingServer::new(dispatcher);

        // Subscribe in background
        let pending = server.pending.clone();
        let poll_id = "test-poll".to_string();

        let waiter_task = tokio::spawn(async move {
            let (tx, rx) = oneshot::channel::<String>();
            {
                let mut p = pending.lock();
                p.entry(poll_id.clone()).or_default().push(tx);
            }
            rx.await
        });

        // Give it a moment to register
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Push event
        server.push_event("test-poll", "hello world".to_string());

        // Wait for result
        let result = tokio::time::timeout(Duration::from_secs(2), waiter_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        assert_eq!(result, "hello world");
    }
}
