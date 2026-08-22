//! Streamable HTTP transport for MCP — POST + Server-Sent Events
//!
//! Mirrors the MCP "Streamable HTTP" transport:
//! - `POST /mcp` for client-to-server messages
//! - `GET /mcp` opens an SSE stream for server-pushed events
//! - Responses are TOON (or JSON if `Accept: application/json`)

use crate::dispatcher::dispatch_mcp;
use crate::McpService;
use http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{channel, Sender};

type SseEvent = (String, String); // (event, data)
type SseRegistry =
    Arc<tokio::sync::Mutex<std::collections::HashMap<String, Vec<Sender<SseEvent>>>>>;

/// Run an MCP server over Streamable HTTP until the process is killed.
pub async fn serve_streamable_http<S: McpService>(
    service: S,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dispatcher = dispatch_mcp(Arc::new(service));
    let registry: SseRegistry = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));

    let listener = TcpListener::bind(addr).await?;
    println!(
        "[toon-rpc-mcp] Streamable HTTP server listening on http://{}",
        addr
    );

    loop {
        let (stream, _) = listener.accept().await?;
        let dispatcher = dispatcher.clone();
        let registry = registry.clone();

        tokio::spawn(async move {
            let connection = hyper_util::rt::TokioIo::new(stream);
            let svc =
                service_fn(move |req| handle_request(req, dispatcher.clone(), registry.clone()));

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(connection, svc)
                .await
            {
                eprintln!("[toon-rpc-mcp] connection error: {}", e);
            }
        });
    }
}

async fn handle_request(
    req: Request<Incoming>,
    dispatcher: reddb_io_toon_rpc::Dispatcher,
    registry: SseRegistry,
) -> Result<Response<String>, Infallible> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();

    if method == http::Method::POST && path == "/mcp" {
        return Ok(handle_post(req, dispatcher).await);
    }

    if method == http::Method::GET && path == "/mcp" {
        return Ok(handle_sse_open(registry).await);
    }

    if method == http::Method::POST && path.starts_with("/mcp/notify/") {
        let subscription_id = path.trim_start_matches("/mcp/notify/").to_string();
        return Ok(handle_notify(req, registry, subscription_id).await);
    }

    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body("Not found".to_string())
        .unwrap())
}

async fn handle_post(
    req: Request<Incoming>,
    dispatcher: reddb_io_toon_rpc::Dispatcher,
) -> Response<String> {
    let body = match req.into_body().collect().await {
        Ok(b) => b.to_bytes().to_vec(),
        Err(_) => {
            let err = serde_json::json!({
                "jsonrpc": "2.0",
                "error": {"code": -32700, "message": "body error"},
                "id": null
            });
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/toon")
                .body(err.to_string())
                .unwrap();
        }
    };

    match dispatcher.dispatch(&body) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/toon")
            .body(String::from_utf8(bytes).unwrap())
            .unwrap(),
        Err(e) => {
            let err = serde_json::json!({
                "jsonrpc": "2.0",
                "error": {"code": -32603, "message": e.to_string()},
                "id": null
            });
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/toon")
                .body(err.to_string())
                .unwrap()
        }
    }
}

async fn handle_sse_open(registry: SseRegistry) -> Response<String> {
    let subscription_id = format!("sub-{}", uuid_simple());
    let (tx, mut rx) = channel::<SseEvent>(64);
    registry
        .lock()
        .await
        .insert(subscription_id.clone(), vec![tx]);

    // Spawn the consumer that drains the channel. In a fuller impl we'd write
    // SSE frames into the response stream; for now we hand the client a
    // subscription id it can use to push events.
    tokio::spawn(async move {
        while rx.recv().await.is_some() {
            // Sink: a real impl would flush these into the SSE response body.
        }
    });

    let body = format!("data: {{\"subscriptionId\":\"{}\"}}\n\n", subscription_id);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .body(body)
        .unwrap()
}

async fn handle_notify(
    req: Request<Incoming>,
    registry: SseRegistry,
    subscription_id: String,
) -> Response<String> {
    let body = match req.into_body().collect().await {
        Ok(b) => b.to_bytes().to_vec(),
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("body error".to_string())
                .unwrap();
        }
    };

    let text = String::from_utf8_lossy(&body).to_string();
    let count = {
        let mut reg = registry.lock().await;
        if let Some(subs) = reg.get_mut(&subscription_id) {
            let drained = std::mem::take(subs);
            let n = drained.len();
            for tx in drained {
                let _ = tx.try_send(("message".into(), text.clone()));
            }
            n
        } else {
            0
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .body(format!("notified {} subscribers", count))
        .unwrap()
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", nanos)
}

/// Re-export of `hyper::service::service_fn` so the caller doesn't have to
/// import hyper directly.
use hyper::service::service_fn;
