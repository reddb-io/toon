//! The TOON-RPC client: pending-call correlation over any transport.
//!
//! Parity target: `packages/toon-rpc/src/client.ts`. A duplex transport gets
//! one receive task that settles pending calls by ID; a request/response
//! transport settles each call from its own response document. A pending call
//! is removed before it settles, whatever settles it (response, timeout,
//! dropped future, transport failure, close), so it settles exactly once.
//! Invalid, unknown-ID and duplicate-ID responses are reported as diagnostics
//! and never settle anything; valid batch siblings still settle.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use reddb_io_toon::{Array, Value as ToonValue};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::error::{Error, RpcError};
use crate::limits::Limits;
use crate::protocol::{Call, Message, Notification, Request, Response};
use crate::serialization::{decode_wire_value, response_from_toon};
use crate::transport::{DuplexTransport, RequestResponseTransport};
use crate::types::{Id, Params, Value};

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientStatus {
    Open,
    Closed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticReason {
    ParseError,
    InvalidResponse,
    UnknownId,
    DuplicateId,
}

impl DiagnosticReason {
    /// The shared corpus spelling of this reason.
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticReason::ParseError => "parse-error",
            DiagnosticReason::InvalidResponse => "invalid-response",
            DiagnosticReason::UnknownId => "unknown-id",
            DiagnosticReason::DuplicateId => "duplicate-id",
        }
    }
}

/// A response document or batch entry the client could not settle.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientDiagnostic {
    pub reason: DiagnosticReason,
    /// The batch entry index, when the problem is inside a batch.
    pub index: Option<usize>,
    pub id: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClientError {
    /// The server answered with an RPC error object.
    #[error(transparent)]
    Rpc(#[from] Error),
    #[error("TOON-RPC call timed out after {0:?}")]
    Timeout(Duration),
    #[error("{0}")]
    Closed(String),
    #[error("TOON-RPC transport failed: {0}")]
    Transport(String),
    /// The transport answered, but not with the matching response.
    #[error("{0}")]
    Protocol(String),
    #[error("invalid TOON-RPC call: {0}")]
    InvalidCall(String),
    /// A client limit refused the call before it was sent.
    #[error("TOON-RPC limit reached: {0}")]
    Limit(String),
}

pub type DiagnosticHandler = Arc<dyn Fn(&ClientDiagnostic) + Send + Sync>;

#[derive(Clone)]
pub struct ClientOptions {
    pub on_diagnostic: Option<DiagnosticHandler>,
    /// Most calls kept pending at once; the next call is refused.
    pub max_pending_calls: usize,
    /// Timeout for a call that sets none of its own.
    pub request_timeout: Option<Duration>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        let limits = Limits::default();
        Self {
            on_diagnostic: None,
            max_pending_calls: limits.max_pending_calls,
            request_timeout: limits.request_timeout,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    /// Use this request ID instead of the next free number.
    pub id: Option<Id>,
    pub timeout: Option<Duration>,
}

type Settle = oneshot::Sender<Result<Value, ClientError>>;

struct State {
    status: ClientStatus,
    terminal: Option<ClientError>,
    pending: HashMap<Id, Settle>,
    next_id: i64,
}

enum Link {
    Duplex(Arc<dyn DuplexTransport>),
    RequestResponse(Arc<dyn RequestResponseTransport>),
}

struct Inner {
    link: Link,
    state: Mutex<State>,
    options: ClientOptions,
    pump: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(pump) = lock(&self.pump).take() {
            pump.abort();
        }
    }
}

#[derive(Clone, Copy)]
enum Scope<'a> {
    /// Any pending call may be settled (a duplex stream).
    Any,
    /// Only this call may be settled (its own request/response exchange).
    Only(&'a Id),
    /// Nothing may be settled (the response to a notification).
    Nothing,
}

/// A TOON-RPC client. Clones share one connection and one pending-call table.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

impl Client {
    /// A client over a duplex transport. Spawns the receive task, so it must
    /// be called inside a Tokio runtime.
    pub fn duplex(transport: impl DuplexTransport, options: ClientOptions) -> Self {
        let transport: Arc<dyn DuplexTransport> = Arc::new(transport);
        let inner = Arc::new(Inner::new(Link::Duplex(transport.clone()), options));
        let pump = tokio::spawn(receive_loop(Arc::downgrade(&inner), transport));
        *lock(&inner.pump) = Some(pump);
        Self { inner }
    }

    /// A client over a request/response transport.
    pub fn request_response(
        transport: impl RequestResponseTransport,
        options: ClientOptions,
    ) -> Self {
        let transport: Arc<dyn RequestResponseTransport> = Arc::new(transport);
        Self {
            inner: Arc::new(Inner::new(Link::RequestResponse(transport), options)),
        }
    }

    pub fn status(&self) -> ClientStatus {
        lock(&self.inner.state).status
    }

    pub fn pending_call_count(&self) -> usize {
        lock(&self.inner.state).pending.len()
    }

    /// Call `method` and wait for its result.
    pub fn call(
        &self,
        method: &str,
        params: Params,
    ) -> impl Future<Output = Result<Value, ClientError>> + Send + 'static {
        self.call_with(method, params, CallOptions::default())
    }

    /// Call `method` with an explicit ID or timeout. The call is registered
    /// as pending before this returns; dropping the future cancels it.
    pub fn call_with(
        &self,
        method: &str,
        params: Params,
        options: CallOptions,
    ) -> impl Future<Output = Result<Value, ClientError>> + Send + 'static {
        let registration = self.inner.register(method, params, options.id);
        // Created now, not on first poll, so a future dropped unpolled still
        // removes its call from the pending table.
        let guard = registration.as_ref().ok().map(|(id, _, _)| PendingGuard {
            inner: self.inner.clone(),
            id: id.clone(),
        });
        let inner = self.inner.clone();
        let timeout = options.timeout.or(self.inner.options.request_timeout);
        async move {
            let _guard = guard;
            let (id, document, settled) = registration?;
            let exchange = async {
                inner.transmit(&id, document).await;
                settled
                    .await
                    .unwrap_or_else(|_| Err(ClientError::Closed(CLOSED.into())))
            };
            match timeout {
                Some(timeout) => tokio::time::timeout(timeout, exchange)
                    .await
                    .unwrap_or(Err(ClientError::Timeout(timeout))),
                None => exchange.await,
            }
        }
    }

    /// Send a notification. No response is expected; any that arrives is
    /// reported as an unknown-ID diagnostic.
    pub async fn notify(&self, method: &str, params: Params) -> Result<(), ClientError> {
        self.inner.assert_open()?;
        let notification = Notification::new(method.to_owned(), params);
        let document = crate::to_wire(&Message::Single(Call::Notification(notification)))
            .map_err(|error| ClientError::InvalidCall(error.to_string()))?;
        match &self.inner.link {
            Link::Duplex(transport) => transport.send(document).await.map_err(transport_failure),
            Link::RequestResponse(transport) => {
                let response = transport
                    .request(document)
                    .await
                    .map_err(transport_failure)?;
                if let Some(response) = response.filter(|response| !response.is_empty()) {
                    self.inner.process(&response, Scope::Nothing);
                }
                Ok(())
            }
        }
    }

    /// Close the client: every pending call is rejected, the receive task
    /// stops, and the transport is released.
    pub async fn close(&self) -> Result<(), ClientError> {
        self.inner
            .terminate(ClientStatus::Closed, ClientError::Closed(CLOSED.into()));
        let pump = lock(&self.inner.pump).take();
        if let Some(pump) = pump {
            pump.abort();
            let _ = pump.await;
        }
        let closed = match &self.inner.link {
            Link::Duplex(transport) => transport.close().await,
            Link::RequestResponse(transport) => transport.close().await,
        };
        closed.map_err(transport_failure)
    }
}

const CLOSED: &str = "TOON-RPC client is closed";

impl Inner {
    fn new(link: Link, options: ClientOptions) -> Self {
        Self {
            link,
            state: Mutex::new(State {
                status: ClientStatus::Open,
                terminal: None,
                pending: HashMap::new(),
                next_id: 0,
            }),
            options,
            pump: Mutex::new(None),
        }
    }

    fn assert_open(&self) -> Result<(), ClientError> {
        let state = lock(&self.state);
        match state.status {
            ClientStatus::Open => Ok(()),
            _ => Err(terminal_error(&state)),
        }
    }

    #[allow(clippy::type_complexity)]
    fn register(
        &self,
        method: &str,
        params: Params,
        id: Option<Id>,
    ) -> Result<(Id, Vec<u8>, oneshot::Receiver<Result<Value, ClientError>>), ClientError> {
        let mut state = lock(&self.state);
        if state.status != ClientStatus::Open {
            return Err(terminal_error(&state));
        }
        if state.pending.len() >= self.options.max_pending_calls {
            return Err(ClientError::Limit(format!(
                "{} calls are already pending",
                self.options.max_pending_calls
            )));
        }
        let id = match id {
            Some(id) => {
                if state.pending.contains_key(&id) {
                    return Err(ClientError::InvalidCall(format!(
                        "call ID is already pending: {id:?}"
                    )));
                }
                id
            }
            None => {
                while state.pending.contains_key(&Id::Number(state.next_id)) {
                    state.next_id += 1;
                }
                if state.next_id > MAX_SAFE_INTEGER {
                    return Err(ClientError::InvalidCall(
                        "numeric ID space exhausted".into(),
                    ));
                }
                state.next_id += 1;
                Id::Number(state.next_id - 1)
            }
        };
        let request = Request::new(method.to_owned(), params, id.clone());
        let document = crate::to_wire(&Message::Single(Call::Request(request)))
            .map_err(|error| ClientError::InvalidCall(error.to_string()))?;
        let (settle, settled) = oneshot::channel();
        state.pending.insert(id.clone(), settle);
        Ok((id, document, settled))
    }

    async fn transmit(&self, id: &Id, document: Vec<u8>) {
        match &self.link {
            Link::Duplex(transport) => {
                if let Err(error) = transport.send(document).await {
                    self.reject(id, transport_failure(error));
                }
            }
            Link::RequestResponse(transport) => match transport.request(document).await {
                Err(error) => self.reject(id, transport_failure(error)),
                Ok(Some(response)) if !response.is_empty() => {
                    self.process(&response, Scope::Only(id));
                    self.reject(
                        id,
                        ClientError::Protocol(
                            "request/response document did not contain the matching response"
                                .into(),
                        ),
                    );
                }
                Ok(_) => self.reject(
                    id,
                    ClientError::Protocol("request/response transport returned no response".into()),
                ),
            },
        }
    }

    fn process(&self, document: &[u8], scope: Scope<'_>) {
        let root = match decode_wire_value(document) {
            Ok(root) => root,
            Err(_) => return self.diagnostic(DiagnosticReason::ParseError, None, None),
        };
        let ToonValue::Array(Array::List(entries)) = root else {
            match response_from_toon(&root) {
                Ok(response) => {
                    self.settle(response, None, scope);
                }
                Err(_) => self.diagnostic(DiagnosticReason::InvalidResponse, None, None),
            }
            return;
        };
        if entries.is_empty() {
            return self.diagnostic(DiagnosticReason::InvalidResponse, None, None);
        }
        let mut settled_ids = HashSet::new();
        for (index, entry) in entries.iter().enumerate() {
            match response_from_toon(entry) {
                Err(_) => self.diagnostic(DiagnosticReason::InvalidResponse, Some(index), None),
                Ok(response) if settled_ids.contains(&response.id) => {
                    let id = response.id;
                    self.diagnostic(DiagnosticReason::DuplicateId, Some(index), Some(id));
                }
                Ok(response) => {
                    let id = response.id.clone();
                    if self.settle(response, Some(index), scope) {
                        settled_ids.insert(id);
                    }
                }
            }
        }
    }

    fn settle(&self, response: Response, index: Option<usize>, scope: Scope<'_>) -> bool {
        let in_scope = match scope {
            Scope::Any => true,
            Scope::Only(id) => *id == response.id,
            Scope::Nothing => false,
        };
        let settle = in_scope
            .then(|| lock(&self.state).pending.remove(&response.id))
            .flatten();
        let Some(settle) = settle else {
            self.diagnostic(DiagnosticReason::UnknownId, index, Some(response.id));
            return false;
        };
        let outcome = match (response.result, response.error) {
            (_, Some(error)) => Err(ClientError::Rpc(error)),
            (Some(result), None) => Ok(result),
            (None, None) => Err(ClientError::Protocol("response has no result".into())),
        };
        let _ = settle.send(outcome);
        true
    }

    fn reject(&self, id: &Id, error: ClientError) {
        let settle = lock(&self.state).pending.remove(id);
        if let Some(settle) = settle {
            let _ = settle.send(Err(error));
        }
    }

    fn terminate(&self, status: ClientStatus, error: ClientError) {
        let pending = {
            let mut state = lock(&self.state);
            if state.status != ClientStatus::Open {
                return;
            }
            state.status = status;
            state.terminal = Some(error.clone());
            std::mem::take(&mut state.pending)
        };
        for settle in pending.into_values() {
            let _ = settle.send(Err(error.clone()));
        }
    }

    fn diagnostic(&self, reason: DiagnosticReason, index: Option<usize>, id: Option<Id>) {
        if let Some(handler) = &self.options.on_diagnostic {
            handler(&ClientDiagnostic { reason, index, id });
        }
    }
}

/// Removes a call from the pending table when its future ends or is dropped.
struct PendingGuard {
    inner: Arc<Inner>,
    id: Id,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        lock(&self.inner.state).pending.remove(&self.id);
    }
}

async fn receive_loop(inner: Weak<Inner>, transport: Arc<dyn DuplexTransport>) {
    loop {
        let next = transport.recv().await;
        let Some(inner) = inner.upgrade() else {
            return;
        };
        match next {
            Ok(Some(document)) => inner.process(&document, Scope::Any),
            Ok(None) => {
                let error = ClientError::Closed("TOON-RPC transport closed".into());
                return inner.terminate(ClientStatus::Closed, error);
            }
            Err(error) => {
                return inner.terminate(ClientStatus::Failed, transport_failure(error));
            }
        }
    }
}

fn terminal_error(state: &State) -> ClientError {
    state
        .terminal
        .clone()
        .unwrap_or_else(|| ClientError::Closed(CLOSED.into()))
}

fn transport_failure(error: RpcError) -> ClientError {
    ClientError::Transport(error.to_string())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
