//! TOON-RPC over HTTP: a request/response exchange, not a duplex stream (§8).
//!
//! Each POST carries one complete RPC document and owns zero or one response
//! document. A request that produces no response (a notification, or a batch
//! of only notifications) is answered with `204 No Content`; every response
//! body is TOON with `Content-Type: application/toon`. Mirrors
//! `packages/toon-rpc/src/http.ts`.

use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use http::{header, Method, Request, Response, StatusCode, Uri};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Body, Incoming};
use hyper_util::client::legacy::{connect::HttpConnector, Client as HyperClient};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use reddb_io_toon_rpc::{
    dispatch_document, serve_until, Dispatcher, Limits, RequestResponseTransport, RpcError,
    Shutdown, DEFAULT_MAX_FRAME_BYTES,
};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

pub const TOON_RPC_CONTENT_TYPE: &str = "application/toon";

/// Largest request (server) or response (client) body accepted by default.
pub const DEFAULT_MAX_BODY_BYTES: usize = DEFAULT_MAX_FRAME_BYTES;

/// Answers one HTTP request; embeddable in any hyper server.
#[derive(Clone)]
pub struct HttpService {
    dispatcher: Dispatcher,
    max_body_bytes: usize,
}

impl HttpService {
    pub fn new(dispatcher: Dispatcher) -> Self {
        Self {
            dispatcher,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
        }
    }

    pub fn with_max_body_bytes(mut self, max_body_bytes: usize) -> Self {
        self.max_body_bytes = max_body_bytes;
        self
    }

    pub async fn handle<B>(&self, request: Request<B>) -> Response<Full<Bytes>>
    where
        B: Body,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        if request.method() != Method::POST {
            return plain(StatusCode::METHOD_NOT_ALLOWED)
                .header(header::ALLOW, "POST")
                .body(Full::default())
                .expect("valid response");
        }
        let body = match Limited::new(request.into_body(), self.max_body_bytes)
            .collect()
            .await
        {
            Ok(body) => body.to_bytes(),
            Err(error) if error.is::<http_body_util::LengthLimitError>() => {
                return empty(StatusCode::PAYLOAD_TOO_LARGE)
            }
            Err(_) => return empty(StatusCode::BAD_REQUEST),
        };
        let document = dispatch_document(&self.dispatcher, &body);
        if document.is_empty() {
            return empty(StatusCode::NO_CONTENT);
        }
        plain(StatusCode::OK)
            .header(header::CONTENT_TYPE, TOON_RPC_CONTENT_TYPE)
            .body(Full::new(Bytes::from(document)))
            .expect("valid response")
    }
}

fn plain(status: StatusCode) -> http::response::Builder {
    Response::builder().status(status)
}

fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    plain(status).body(Full::default()).expect("valid response")
}

/// Serve one HTTP/1.1 connection until it ends or shutdown is requested; on
/// shutdown the connection finishes the exchange in progress and closes.
/// `idle_timeout` bounds how long a connection may wait for request headers.
pub async fn serve_http_connection<S, B>(
    stream: TcpStream,
    service: S,
    idle_timeout: Option<Duration>,
    mut shutdown: Shutdown,
) where
    S: hyper::service::Service<Request<Incoming>, Response = Response<B>, Error = Infallible>,
    S::Future: Send + 'static,
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder.timer(TokioTimer::new());
    builder.header_read_timeout(idle_timeout);
    let connection = builder.serve_connection(TokioIo::new(stream), service);
    tokio::pin!(connection);
    tokio::select! {
        _ = connection.as_mut() => {}
        _ = shutdown.requested() => {
            connection.as_mut().graceful_shutdown();
            let _ = connection.await;
        }
    }
}

/// An HTTP/1.1 server bound to an address the caller chose.
pub struct HttpServer {
    listener: TcpListener,
    service: HttpService,
    limits: Limits,
}

impl HttpServer {
    pub async fn bind(addr: impl ToSocketAddrs, dispatcher: Dispatcher) -> io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self::from_listener(listener, HttpService::new(dispatcher)))
    }

    pub fn from_listener(listener: TcpListener, service: HttpService) -> Self {
        Self {
            listener,
            service,
            limits: Limits::default(),
        }
    }

    /// Also applies `max_body_bytes` and `max_batch_length` to the service.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.service.max_body_bytes = limits.max_body_bytes;
        self.service.dispatcher = self
            .service
            .dispatcher
            .with_max_batch_length(limits.max_batch_length);
        self.limits = limits;
        self
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Serve until accepting fails.
    pub async fn serve(self) -> io::Result<()> {
        self.serve_with_shutdown(std::future::pending()).await
    }

    /// Serve until `signal` resolves, then let open connections finish the
    /// exchange in progress and close, within the shutdown grace period.
    pub async fn serve_with_shutdown(self, signal: impl Future<Output = ()>) -> io::Result<()> {
        let Self {
            listener,
            service,
            limits,
        } = self;
        serve_until(
            limits.max_connections,
            limits.shutdown_grace,
            || async { listener.accept().await.map(|(stream, _)| stream) },
            |stream, shutdown| {
                let service = service.clone();
                let handler = hyper::service::service_fn(move |request: Request<Incoming>| {
                    let service = service.clone();
                    async move { Ok::<_, Infallible>(service.handle(request).await) }
                });
                serve_http_connection(stream, handler, limits.idle_timeout, shutdown)
            },
            signal,
        )
        .await
    }
}

/// A request/response client transport over plain HTTP (`http://`), with
/// pooled connections. For TLS, implement `RequestResponseTransport` over an
/// HTTPS client instead.
pub struct HttpTransport {
    uri: Uri,
    client: HyperClient<HttpConnector, Full<Bytes>>,
    max_body_bytes: usize,
}

impl HttpTransport {
    pub fn new(uri: Uri) -> Self {
        Self {
            uri,
            client: HyperClient::builder(TokioExecutor::new()).build_http(),
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
        }
    }

    pub fn with_max_body_bytes(mut self, max_body_bytes: usize) -> Self {
        self.max_body_bytes = max_body_bytes;
        self
    }
}

#[async_trait::async_trait]
impl RequestResponseTransport for HttpTransport {
    async fn request(&self, document: Vec<u8>) -> Result<Option<Vec<u8>>, RpcError> {
        let request = Request::post(self.uri.clone())
            .header(header::CONTENT_TYPE, TOON_RPC_CONTENT_TYPE)
            .header(header::ACCEPT, TOON_RPC_CONTENT_TYPE)
            .body(Full::new(Bytes::from(document)))
            .map_err(transport_error)?;
        let response = self
            .client
            .request(request)
            .await
            .map_err(transport_error)?;
        let status = response.status();
        let body = Limited::new(response.into_body(), self.max_body_bytes)
            .collect()
            .await
            .map_err(|error| RpcError::TransportError(error.to_string()))?
            .to_bytes();
        if !status.is_success() {
            return Err(RpcError::TransportError(format!(
                "TOON-RPC HTTP request failed: {status}"
            )));
        }
        if status == StatusCode::NO_CONTENT || body.is_empty() {
            return Ok(None);
        }
        Ok(Some(body.to_vec()))
    }
}

fn transport_error(error: impl std::fmt::Display) -> RpcError {
    RpcError::TransportError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reddb_io_toon_rpc::{Client, ClientError, ClientOptions, ErrorCode, Params};
    use serde_json::json;

    async fn server(max_body_bytes: usize) -> Uri {
        let mut dispatcher = Dispatcher::new();
        dispatcher.register("echo", |params, _id| match params {
            Params::ByPosition(mut values) if !values.is_empty() => Ok(values.remove(0)),
            _ => Ok(json!(null)),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let service = HttpService::new(dispatcher).with_max_body_bytes(max_body_bytes);
        tokio::spawn(HttpServer::from_listener(listener, service).serve());
        format!("http://{addr}/rpc").parse().unwrap()
    }

    async fn post(uri: &Uri, method: Method, body: &'static str) -> (StatusCode, String, Bytes) {
        let client = HyperClient::builder(TokioExecutor::new()).build_http();
        let request = Request::builder()
            .method(method)
            .uri(uri.clone())
            .body(Full::new(Bytes::from(body)))
            .unwrap();
        let response = client.request(request).await.unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|value| value.to_str().unwrap().to_owned())
            .unwrap_or_default();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, content_type, body)
    }

    #[tokio::test]
    async fn calls_round_trip_through_the_client() {
        let client = Client::request_response(
            HttpTransport::new(server(DEFAULT_MAX_BODY_BYTES).await),
            ClientOptions::default(),
        );
        let text = "multi\n\nline";
        let result = client
            .call("echo", Params::ByPosition(vec![json!(text)]))
            .await
            .unwrap();
        assert_eq!(result, json!(text));
        let missing = client.call("missing", Params::Absent).await.unwrap_err();
        assert!(
            matches!(missing, ClientError::Rpc(error) if error.code == ErrorCode::MethodNotFound)
        );
        client.notify("echo", Params::Absent).await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_stops_accepting_and_limits_apply_to_the_service() {
        let (trigger, signal) = tokio::sync::oneshot::channel::<()>();
        let server = HttpServer::bind("127.0.0.1:0", Dispatcher::new())
            .await
            .unwrap()
            .with_limits(Limits {
                max_body_bytes: 8,
                ..Limits::default()
            });
        let uri: Uri = format!("http://{}/", server.local_addr().unwrap())
            .parse()
            .unwrap();
        let serving = tokio::spawn(server.serve_with_shutdown(async {
            let _ = signal.await;
        }));
        let (status, _, _) = post(&uri, Method::POST, "toonrpc: \"1.0\"\nmethod: m").await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        trigger.send(()).unwrap();
        serving.await.unwrap().unwrap();
        let transport = HttpTransport::new(uri);
        assert!(transport.request(b"x".to_vec()).await.is_err());
    }

    #[tokio::test]
    async fn a_notification_is_answered_with_no_content() {
        let uri = server(DEFAULT_MAX_BODY_BYTES).await;
        let (status, _, body) = post(&uri, Method::POST, "toonrpc: \"1.0\"\nmethod: echo").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn errors_are_toon_documents() {
        let uri = server(DEFAULT_MAX_BODY_BYTES).await;
        let (status, content_type, body) = post(&uri, Method::POST, "\"unterminated").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type, TOON_RPC_CONTENT_TYPE);
        let response = reddb_io_toon_rpc::response_from_wire(&body).unwrap();
        assert_eq!(response.error.unwrap().code, ErrorCode::ParseError);
    }

    #[tokio::test]
    async fn oversized_bodies_and_other_methods_are_refused() {
        let uri = server(8).await;
        let (status, _, _) = post(&uri, Method::POST, "toonrpc: \"1.0\"\nmethod: echo").await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        let (status, _, _) = post(&uri, Method::GET, "").await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    }
}
