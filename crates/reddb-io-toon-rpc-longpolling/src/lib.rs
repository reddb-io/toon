//! Experimental TOON-RPC long polling. Unpublished: spec §9 defers long
//! polling, so this crate has no transport profile and no client.
//!
//! `POST /rpc` is a plain request/response exchange (204 when there is no
//! response). `GET /poll/{id}` waits up to the poll timeout for an event the
//! host application pushes in-process with `push_event`. Events can only be
//! pushed from inside the process; there is no HTTP route for it. The waiter
//! table is bounded in keys and in waiters per key, and a key is removed as
//! soon as its last waiter leaves.

use std::collections::HashMap;
use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use http::{header, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Body, Incoming};
use hyper_util::rt::TokioIo;
use reddb_io_toon_rpc::{dispatch_document, Dispatcher, DEFAULT_MAX_FRAME_BYTES};
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::oneshot;

pub const DEFAULT_POLL_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_POLL_KEYS: usize = 1024;
pub const DEFAULT_MAX_WAITERS_PER_KEY: usize = 16;

type Waiters = Arc<Mutex<HashMap<String, Vec<(u64, oneshot::Sender<Bytes>)>>>>;

#[derive(Clone)]
pub struct LongPollingService {
    dispatcher: Dispatcher,
    waiters: Waiters,
    next_waiter: Arc<std::sync::atomic::AtomicU64>,
    poll_timeout: Duration,
    max_poll_keys: usize,
    max_waiters_per_key: usize,
    max_body_bytes: usize,
}

impl LongPollingService {
    pub fn new(dispatcher: Dispatcher) -> Self {
        Self {
            dispatcher,
            waiters: Arc::default(),
            next_waiter: Arc::default(),
            poll_timeout: DEFAULT_POLL_TIMEOUT,
            max_poll_keys: DEFAULT_MAX_POLL_KEYS,
            max_waiters_per_key: DEFAULT_MAX_WAITERS_PER_KEY,
            max_body_bytes: DEFAULT_MAX_FRAME_BYTES,
        }
    }

    pub fn with_poll_timeout(mut self, poll_timeout: Duration) -> Self {
        self.poll_timeout = poll_timeout;
        self
    }

    pub fn with_limits(mut self, max_poll_keys: usize, max_waiters_per_key: usize) -> Self {
        self.max_poll_keys = max_poll_keys;
        self.max_waiters_per_key = max_waiters_per_key;
        self
    }

    /// Deliver `event` to every current waiter of `poll_id`; returns how many
    /// received it.
    pub fn push_event(&self, poll_id: &str, event: impl Into<Bytes>) -> usize {
        let waiters = lock(&self.waiters).remove(poll_id).unwrap_or_default();
        let event = event.into();
        waiters
            .into_iter()
            .filter_map(|(_, waiter)| waiter.send(event.clone()).ok())
            .count()
    }

    /// Poll keys with at least one waiter.
    pub fn poll_key_count(&self) -> usize {
        lock(&self.waiters).len()
    }

    pub async fn handle<B>(&self, request: Request<B>) -> Response<Full<Bytes>>
    where
        B: Body,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        let path = request.uri().path().to_owned();
        match (request.method(), path.as_str()) {
            (&Method::POST, "/rpc") => self.rpc(request.into_body()).await,
            (&Method::GET, path) => match path.strip_prefix("/poll/") {
                Some(poll_id) if !poll_id.is_empty() => self.poll(poll_id.to_owned()).await,
                _ => empty(StatusCode::NOT_FOUND),
            },
            _ => empty(StatusCode::NOT_FOUND),
        }
    }

    async fn rpc<B>(&self, body: B) -> Response<Full<Bytes>>
    where
        B: Body,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        let body = match Limited::new(body, self.max_body_bytes).collect().await {
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
        toon(document.into())
    }

    async fn poll(&self, poll_id: String) -> Response<Full<Bytes>> {
        let (sender, receiver) = oneshot::channel();
        let waiter = self
            .next_waiter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut waiters = lock(&self.waiters);
            if !waiters.contains_key(&poll_id) && waiters.len() >= self.max_poll_keys {
                return empty(StatusCode::SERVICE_UNAVAILABLE);
            }
            let entry = waiters.entry(poll_id.clone()).or_default();
            if entry.len() >= self.max_waiters_per_key {
                return empty(StatusCode::SERVICE_UNAVAILABLE);
            }
            entry.push((waiter, sender));
        }
        // Leaves the table however the wait ends, including a dropped request.
        let _guard = WaiterGuard {
            waiters: self.waiters.clone(),
            poll_id,
            waiter,
        };
        match tokio::time::timeout(self.poll_timeout, receiver).await {
            Ok(Ok(event)) => toon(event),
            _ => empty(StatusCode::NO_CONTENT),
        }
    }
}

struct WaiterGuard {
    waiters: Waiters,
    poll_id: String,
    waiter: u64,
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        let mut waiters = lock(&self.waiters);
        if let Some(entry) = waiters.get_mut(&self.poll_id) {
            entry.retain(|(waiter, _)| *waiter != self.waiter);
            if entry.is_empty() {
                waiters.remove(&self.poll_id);
            }
        }
    }
}

fn toon(body: Bytes) -> Response<Full<Bytes>> {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/toon")
        .body(Full::new(body))
        .expect("valid response")
}

fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::default())
        .expect("valid response")
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A long-polling server bound to an address the caller chose.
pub struct LongPollingServer {
    listener: TcpListener,
    service: LongPollingService,
}

impl LongPollingServer {
    pub async fn bind(addr: impl ToSocketAddrs, dispatcher: Dispatcher) -> io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self::from_listener(
            listener,
            LongPollingService::new(dispatcher),
        ))
    }

    pub fn from_listener(listener: TcpListener, service: LongPollingService) -> Self {
        Self { listener, service }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// The service, for pushing events while the server runs.
    pub fn service(&self) -> LongPollingService {
        self.service.clone()
    }

    pub async fn serve(self) -> io::Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let service = self.service.clone();
            tokio::spawn(async move {
                let handler = hyper::service::service_fn(move |request: Request<Incoming>| {
                    let service = service.clone();
                    async move { Ok::<_, Infallible>(service.handle(request).await) }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), handler)
                    .await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(path: &str) -> Request<Full<Bytes>> {
        Request::get(path).body(Full::default()).unwrap()
    }

    #[tokio::test]
    async fn a_pushed_event_reaches_every_waiter_and_clears_the_key() {
        let service = LongPollingService::new(Dispatcher::new());
        let waiting = (0..2)
            .map(|_| {
                let service = service.clone();
                tokio::spawn(async move { service.handle(get("/poll/k")).await })
            })
            .collect::<Vec<_>>();
        while lock(&service.waiters).get("k").map_or(0, Vec::len) < 2 {
            tokio::task::yield_now().await;
        }
        assert_eq!(service.push_event("k", "event"), 2);
        for response in waiting {
            let body = response.await.unwrap().into_body().collect().await.unwrap();
            assert_eq!(body.to_bytes(), "event");
        }
        assert_eq!(service.poll_key_count(), 0);
    }

    #[tokio::test]
    async fn waiters_leave_on_timeout_and_the_table_is_bounded() {
        let service = LongPollingService::new(Dispatcher::new())
            .with_poll_timeout(Duration::from_millis(20))
            .with_limits(1, 1);
        let first = {
            let service = service.clone();
            tokio::spawn(async move { service.handle(get("/poll/a")).await })
        };
        while service.poll_key_count() == 0 {
            tokio::task::yield_now().await;
        }
        let refused = [
            service.handle(get("/poll/b")).await.status(),
            service.handle(get("/poll/a")).await.status(),
        ];
        assert_eq!(refused, [StatusCode::SERVICE_UNAVAILABLE; 2]);
        assert_eq!(first.await.unwrap().status(), StatusCode::NO_CONTENT);
        assert_eq!(service.poll_key_count(), 0);
    }

    #[tokio::test]
    async fn there_is_no_http_route_to_push_events() {
        let service = LongPollingService::new(Dispatcher::new());
        let notify = Request::post("/notify/k")
            .body(Full::<Bytes>::default())
            .unwrap();
        assert_eq!(service.handle(notify).await.status(), StatusCode::NOT_FOUND);
    }
}
