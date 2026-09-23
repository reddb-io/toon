//! TOON-RPC over Server-Sent Events: the §8.2 duplex profile.
//!
//! A client opens one long-lived `GET` event stream and sends each document as
//! a `POST`. The server acknowledges a POST with `202 Accepted` and delivers
//! the response document, when there is one, as a single `data:` event on
//! that client's stream. Both legs name the same session with a `session`
//! query parameter the client chooses; the session ID is the only thing that
//! ties a POST to a stream, so it must be unguessable. Closing the stream ends
//! the session. Mirrors `packages/toon-rpc/src/sse.ts`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::StreamExt;
use http::{header, Method, Request, Response, StatusCode, Uri};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full, Limited, StreamBody};
use hyper::body::{Body, Frame, Incoming};
use hyper_util::client::legacy::{connect::HttpConnector, Client as HyperClient};
use hyper_util::rt::TokioExecutor;
use reddb_io_toon_rpc::{
    dispatch_document, serve_until, Dispatcher, DuplexTransport, Limits, RpcError,
    DEFAULT_MAX_FRAME_BYTES,
};
use reddb_io_toon_rpc_http::serve_http_connection;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::mpsc;

pub const TOON_RPC_CONTENT_TYPE: &str = "application/toon";
pub const EVENT_STREAM_CONTENT_TYPE: &str = "text/event-stream";

/// Largest POST body (server) or event (client) accepted by default.
pub const DEFAULT_MAX_BODY_BYTES: usize = DEFAULT_MAX_FRAME_BYTES;

/// Responses buffered per session before a POST waits for the stream.
pub const DEFAULT_EVENT_QUEUE: usize = 64;

type Sessions = Arc<Mutex<HashMap<String, mpsc::Sender<Bytes>>>>;
type ResponseBody = BoxBody<Bytes, Infallible>;

/// Encode one document as one SSE event: each line becomes a `data:` line.
pub fn encode_event(document: &[u8]) -> Vec<u8> {
    let mut event = Vec::with_capacity(document.len() + 16);
    for line in document.split(|&byte| byte == b'\n') {
        event.extend_from_slice(b"data: ");
        event.extend_from_slice(line);
        event.push(b'\n');
    }
    event.push(b'\n');
    event
}

/// Answers the two legs of the SSE profile; embeddable in any hyper server.
#[derive(Clone)]
pub struct SseService {
    dispatcher: Dispatcher,
    sessions: Sessions,
    max_body_bytes: usize,
    event_queue: usize,
}

impl SseService {
    pub fn new(dispatcher: Dispatcher) -> Self {
        Self {
            dispatcher,
            sessions: Arc::default(),
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            event_queue: DEFAULT_EVENT_QUEUE,
        }
    }

    pub fn with_max_body_bytes(mut self, max_body_bytes: usize) -> Self {
        self.max_body_bytes = max_body_bytes;
        self
    }

    pub fn with_event_queue(mut self, event_queue: usize) -> Self {
        self.event_queue = event_queue.max(1);
        self
    }

    /// Sessions with an open event stream.
    pub fn session_count(&self) -> usize {
        lock(&self.sessions).len()
    }

    /// End every open event stream (after the events already queued).
    pub fn close_sessions(&self) {
        lock(&self.sessions).clear();
    }

    pub async fn handle<B>(&self, request: Request<B>) -> Response<ResponseBody>
    where
        B: Body,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        let Some(session) = session_id(request.uri()) else {
            return empty(StatusCode::BAD_REQUEST);
        };
        match *request.method() {
            Method::GET => self.open(session),
            Method::POST => self.accept(session, request.into_body()).await,
            _ => Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .header(header::ALLOW, "GET, POST")
                .body(Empty::new().boxed())
                .expect("valid response"),
        }
    }

    fn open(&self, session: String) -> Response<ResponseBody> {
        let (sender, receiver) = mpsc::channel(self.event_queue);
        {
            let mut sessions = lock(&self.sessions);
            if sessions.contains_key(&session) {
                return empty(StatusCode::CONFLICT);
            }
            sessions.insert(session.clone(), sender);
        }
        let guard = SessionGuard {
            sessions: self.sessions.clone(),
            session,
        };
        // A comment first, so the client sees the stream open immediately.
        let opening = futures::stream::once(async { Bytes::from_static(b": open\n\n") });
        let events = futures::stream::unfold((receiver, guard), |(mut receiver, guard)| async {
            receiver
                .recv()
                .await
                .map(|event| (event, (receiver, guard)))
        });
        let body = StreamBody::new(
            opening
                .chain(events)
                .map(|event| Ok::<_, Infallible>(Frame::data(event))),
        );
        Response::builder()
            .header(header::CONTENT_TYPE, EVENT_STREAM_CONTENT_TYPE)
            .header(header::CACHE_CONTROL, "no-cache")
            .body(BodyExt::boxed(body))
            .expect("valid response")
    }

    async fn accept<B>(&self, session: String, body: B) -> Response<ResponseBody>
    where
        B: Body,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        let Some(stream) = lock(&self.sessions).get(&session).cloned() else {
            return empty(StatusCode::NOT_FOUND);
        };
        let body = match Limited::new(body, self.max_body_bytes).collect().await {
            Ok(body) => body.to_bytes(),
            Err(error) if error.is::<http_body_util::LengthLimitError>() => {
                return empty(StatusCode::PAYLOAD_TOO_LARGE)
            }
            Err(_) => return empty(StatusCode::BAD_REQUEST),
        };
        let document = dispatch_document(&self.dispatcher, &body);
        // Waiting for room in the queue is the backpressure; a stream that
        // closed in the meantime cannot take the response.
        if !document.is_empty() && stream.send(encode_event(&document).into()).await.is_err() {
            return empty(StatusCode::GONE);
        }
        empty(StatusCode::ACCEPTED)
    }
}

/// Removes a session when its event stream is dropped.
struct SessionGuard {
    sessions: Sessions,
    session: String,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        lock(&self.sessions).remove(&self.session);
    }
}

fn session_id(uri: &Uri) -> Option<String> {
    uri.query()?
        .split('&')
        .find_map(|pair| pair.strip_prefix("session="))
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

fn empty(status: StatusCode) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .body(Empty::new().boxed())
        .expect("valid response")
}

/// An SSE server bound to an address the caller chose.
pub struct SseServer {
    listener: TcpListener,
    service: SseService,
    limits: Limits,
}

impl SseServer {
    pub async fn bind(addr: impl ToSocketAddrs, dispatcher: Dispatcher) -> io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self::from_listener(listener, SseService::new(dispatcher)))
    }

    pub fn from_listener(listener: TcpListener, service: SseService) -> Self {
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

    /// Serve until `signal` resolves. Every open event stream then ends, and
    /// connections finish the exchange in progress within the grace period.
    pub async fn serve_with_shutdown(self, signal: impl Future<Output = ()>) -> io::Result<()> {
        let Self {
            listener,
            service,
            limits,
        } = self;
        let closing = service.clone();
        let signal = async move {
            signal.await;
            closing.close_sessions();
        };
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

/// A duplex client transport over plain HTTP (`http://`): one event stream
/// in, one POST per document out, under a freshly generated session.
pub struct SseTransport {
    session_uri: Uri,
    client: HyperClient<HttpConnector, Full<Bytes>>,
    events: tokio::sync::Mutex<EventReader>,
}

impl SseTransport {
    /// Open the event stream at `uri`; documents are POSTed to the same URI.
    pub async fn connect(uri: Uri) -> Result<Self, RpcError> {
        Self::connect_with_max_event_bytes(uri, DEFAULT_MAX_BODY_BYTES).await
    }

    pub async fn connect_with_max_event_bytes(
        uri: Uri,
        max_event_bytes: usize,
    ) -> Result<Self, RpcError> {
        let session_uri = with_session(&uri, &new_session_id())?;
        let client = HyperClient::builder(TokioExecutor::new()).build_http();
        let request = Request::get(session_uri.clone())
            .header(header::ACCEPT, EVENT_STREAM_CONTENT_TYPE)
            .body(Full::default())
            .map_err(transport_error)?;
        let response = client.request(request).await.map_err(transport_error)?;
        if !response.status().is_success() {
            return Err(RpcError::TransportError(format!(
                "TOON-RPC SSE stream failed: {}",
                response.status()
            )));
        }
        Ok(Self {
            session_uri,
            client,
            events: tokio::sync::Mutex::new(EventReader {
                body: Some(response.into_body()),
                parser: EventParser::new(max_event_bytes),
            }),
        })
    }
}

#[async_trait::async_trait]
impl DuplexTransport for SseTransport {
    async fn send(&self, document: Vec<u8>) -> Result<(), RpcError> {
        let request = Request::post(self.session_uri.clone())
            .header(header::CONTENT_TYPE, TOON_RPC_CONTENT_TYPE)
            .body(Full::new(Bytes::from(document)))
            .map_err(transport_error)?;
        let response = self
            .client
            .request(request)
            .await
            .map_err(transport_error)?;
        let status = response.status();
        let _ = response.into_body().collect().await;
        if !status.is_success() {
            return Err(RpcError::TransportError(format!(
                "TOON-RPC SSE request failed: {status}"
            )));
        }
        Ok(())
    }

    async fn recv(&self) -> Result<Option<Vec<u8>>, RpcError> {
        self.events.lock().await.next_event().await
    }

    async fn close(&self) -> Result<(), RpcError> {
        // Dropping the response body closes the stream and ends the session.
        self.events.lock().await.body = None;
        Ok(())
    }
}

struct EventReader {
    body: Option<Incoming>,
    parser: EventParser,
}

impl EventReader {
    async fn next_event(&mut self) -> Result<Option<Vec<u8>>, RpcError> {
        loop {
            if let Some(event) = self.parser.next_event() {
                return Ok(Some(event));
            }
            let Some(body) = self.body.as_mut() else {
                return Ok(None);
            };
            match body.frame().await {
                None => {
                    self.body = None;
                    return Ok(None);
                }
                Some(Err(error)) => return Err(transport_error(error)),
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        self.parser.push(&data)?;
                    }
                }
            }
        }
    }
}

/// The subset of the SSE grammar the profile needs: complete events whose
/// `data:` lines rejoin with LF. Comments and other fields are ignored.
struct EventParser {
    buffer: Vec<u8>,
    data: Option<Vec<u8>>,
    ready: std::collections::VecDeque<Vec<u8>>,
    max_event_bytes: usize,
}

impl EventParser {
    fn new(max_event_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            data: None,
            ready: Default::default(),
            max_event_bytes,
        }
    }

    fn next_event(&mut self) -> Option<Vec<u8>> {
        self.ready.pop_front()
    }

    fn push(&mut self, chunk: &[u8]) -> Result<(), RpcError> {
        self.buffer.extend_from_slice(chunk);
        while let Some(end) = self.buffer.iter().position(|&byte| byte == b'\n') {
            let mut line = self.buffer.drain(..=end).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.line(&line);
        }
        let pending = self.buffer.len() + self.data.as_ref().map_or(0, Vec::len);
        if pending > self.max_event_bytes {
            return Err(RpcError::TransportError(
                "TOON-RPC SSE event exceeds the size limit".into(),
            ));
        }
        Ok(())
    }

    fn line(&mut self, line: &[u8]) {
        if line.is_empty() {
            if let Some(data) = self.data.take() {
                self.ready.push_back(data);
            }
            return;
        }
        let (field, value) = match line.iter().position(|&byte| byte == b':') {
            Some(0) => return,
            Some(colon) => (&line[..colon], &line[colon + 1..]),
            None => (line, &[][..]),
        };
        if field != b"data" {
            return;
        }
        let value = value.strip_prefix(b" ").unwrap_or(value);
        match &mut self.data {
            Some(data) => {
                data.push(b'\n');
                data.extend_from_slice(value);
            }
            None => self.data = Some(value.to_vec()),
        }
    }
}

fn with_session(uri: &Uri, session: &str) -> Result<Uri, RpcError> {
    let path_and_query = match uri.path_and_query() {
        Some(existing) => match existing.query() {
            Some(_) => format!("{existing}&session={session}"),
            None => format!("{existing}?session={session}"),
        },
        None => format!("/?session={session}"),
    };
    let mut parts = uri.clone().into_parts();
    parts.path_and_query = Some(path_and_query.parse().map_err(transport_error)?);
    Uri::from_parts(parts).map_err(transport_error)
}

/// 128 bits from the process's randomly keyed SipHash, as 32 hex digits.
fn new_session_id() -> String {
    let state = std::collections::hash_map::RandomState::new();
    let half = |salt: u64| {
        let mut hasher = state.build_hasher();
        hasher.write_u64(salt);
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos()),
        );
        hasher.finish()
    };
    format!("{:016x}{:016x}", half(1), half(2))
}

fn transport_error(error: impl std::fmt::Display) -> RpcError {
    RpcError::TransportError(error.to_string())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
