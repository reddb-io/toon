//! MCP over HTTP POST — request/response only.
//!
//! # What this is, and what it is not
//!
//! This is **not** the full Streamable HTTP transport of the pinned revision.
//! It implements only the half that is a plain request/response exchange:
//!
//! - `POST /mcp` with a JSON-RPC request body returns a JSON-RPC response body
//!   as `application/json`.
//!
//! It does **not** implement Server-Sent Events, `subscriptions/listen`
//! streams, resumability, or session headers. A previous revision of this file
//! answered `GET /mcp` with an SSE content type and then dropped every event
//! into an unread channel; that route is gone rather than left to look like a
//! working stream. Clients needing server-initiated messages must use stdio.
//!
//! The `MCP-Protocol-Version` header is read and echoed, but version
//! enforcement happens in the dispatcher against the request's `_meta`, so that
//! stdio and HTTP agree on one rule.

use crate::dispatcher::McpDispatcher;
use crate::jsonrpc::{to_line, JsonRpcError, JsonRpcResponse, INVALID_REQUEST, PARSE_ERROR};
use crate::types::MCP_PROTOCOL_VERSION;
use crate::McpService;
use http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::service::service_fn;
use serde_json::Value;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Maximum accepted request body, so a peer cannot force unbounded buffering.
pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Serve MCP over HTTP POST until the process is stopped.
pub async fn serve_http_post<S: McpService>(
    service: S,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dispatcher = Arc::new(McpDispatcher::new(Arc::new(service)));
    let listener = TcpListener::bind(addr).await?;
    eprintln!("[toon-rpc-mcp] HTTP POST endpoint listening on http://{addr}/mcp");

    loop {
        let (stream, _) = listener.accept().await?;
        let dispatcher = dispatcher.clone();

        tokio::spawn(async move {
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = service_fn(move |req| handle(req, dispatcher.clone()));
            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, svc)
                .await
            {
                eprintln!("[toon-rpc-mcp] connection error: {e}");
            }
        });
    }
}

async fn handle<S: McpService>(
    req: Request<Incoming>,
    dispatcher: Arc<McpDispatcher<S>>,
) -> Result<Response<String>, Infallible> {
    if req.method() != http::Method::POST || req.uri().path() != "/mcp" {
        return Ok(plain(
            StatusCode::NOT_FOUND,
            "Not found. This endpoint serves POST /mcp only; it does not implement SSE.",
        ));
    }

    let collected = match req.into_body().collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => return Ok(json_error(PARSE_ERROR, "Parse error: could not read body")),
    };

    if collected.len() > MAX_BODY_BYTES {
        return Ok(json_error(
            INVALID_REQUEST,
            format!("Invalid Request: body exceeds {MAX_BODY_BYTES} bytes"),
        ));
    }

    let text = match std::str::from_utf8(&collected) {
        Ok(t) => t,
        Err(_) => return Ok(json_error(PARSE_ERROR, "Parse error: body is not UTF-8")),
    };

    Ok(match dispatcher.handle_line(text) {
        // A notification gets 202 with no body, since there is nothing to send.
        None => Response::builder()
            .status(StatusCode::ACCEPTED)
            .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
            .body(String::new())
            .unwrap(),
        Some(body) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
            .body(body)
            .unwrap(),
    })
}

fn json_error(code: i32, message: impl Into<String>) -> Response<String> {
    let body = to_line(&JsonRpcResponse::failure(
        Value::Null,
        JsonRpcError::new(code, message),
    ));
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap()
}

fn plain(status: StatusCode, message: &str) -> Response<String> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(message.to_string())
        .unwrap()
}
