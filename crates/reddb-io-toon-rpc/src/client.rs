//! Production TOON-RPC client.
//!
//! The client owns pending-call correlation, the lifecycle, and settlement. It
//! keeps a pending map keyed by the *typed* [`Id`], so `1`, `"1"` and `null`
//! are three different calls, and it settles every pending call exactly once:
//! on a matching response, an RPC error, a timeout, a cancellation, a send
//! failure, transport failure or completion, or [`Client::close`].
//!
//! Response documents that cannot settle a call are never silently dropped.
//! Unparsable documents, invalid envelopes, unknown IDs, and duplicate IDs
//! within one batch are surfaced as [`ClientDiagnostic`]s, and a valid batch
//! sibling still settles its own call.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reddb_io_toon::{Array as ToonArray, Value as ToonValue};
use tokio::sync::{mpsc, oneshot, OnceCell};
use tokio::task::JoinHandle;

use crate::cancel::CancelToken;
use crate::error::Error as ErrorObject;
use crate::protocol::{Call, Message, Notification, Request, Response};
use crate::serialization::{decode_wire_value, response_from_value, to_wire};
use crate::transport::{DuplexTransport, RequestResponseTransport, TransportError};
use crate::types::{Id, Params, Value};

/// Largest ID the client allocates; beyond it a value is not exactly
/// representable in every conforming implementation.
const MAX_SAFE_ID: i64 = 9_007_199_254_740_991;

/// Client lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientStatus {
    /// Constructed; the transport has not been opened.
    Idle,
    /// The transport is opening.
    Opening,
    /// The transport is open and calls can be dispatched.
    Open,
    /// Terminated by [`Client::close`] or a clean transport end.
    Closed,
    /// Terminated by a transport failure.
    Failed,
}

/// Why a received document could not settle a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticReason {
    /// The document is not decodable TOON, or not valid UTF-8.
    ParseError,
    /// The document decoded but is not a valid response envelope.
    InvalidResponse,
    /// The envelope is valid but its ID matches no pending call.
    UnknownId,
    /// A later batch entry repeats an ID already settled by this batch.
    DuplicateId,
}

impl DiagnosticReason {
    /// The stable wire name used by the shared conformance corpus.
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticReason::ParseError => "parse-error",
            DiagnosticReason::InvalidResponse => "invalid-response",
            DiagnosticReason::UnknownId => "unknown-id",
            DiagnosticReason::DuplicateId => "duplicate-id",
        }
    }
}

/// One observable rejection of a received document or batch entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientDiagnostic {
    /// Why the document or entry was rejected.
    pub reason: DiagnosticReason,
    /// Position within a batch document, absent for a single document.
    pub index: Option<usize>,
    /// The rejected envelope's ID, when one was recoverable.
    pub id: Option<Id>,
}

/// A client-side failure that is not an RPC error object.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClientError {
    /// The peer answered with an RPC error object.
    #[error(transparent)]
    Rpc(#[from] ErrorObject),
    /// The client is closed, or was closed while the call was in flight.
    #[error("TOON-RPC client is closed: {0}")]
    Closed(String),
    /// The caller cancelled the operation.
    #[error("TOON-RPC operation was aborted")]
    Aborted,
    /// The per-call deadline elapsed before a response arrived.
    #[error("TOON-RPC call timed out after {0}ms")]
    Timeout(u128),
    /// The peer or transport broke the correlation contract.
    #[error("TOON-RPC protocol error: {0}")]
    Protocol(String),
    /// The transport failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The call could not be encoded as a valid request.
    #[error("invalid TOON-RPC request: {0}")]
    InvalidRequest(String),
}

/// Per-call knobs: explicit ID, deadline, and cancellation.
#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    /// Correlate on this exact ID instead of an allocated one.
    pub id: Option<Id>,
    /// Reject the call once this much time has elapsed.
    pub timeout: Option<Duration>,
    /// Reject the call when this token is cancelled.
    pub cancel: Option<CancelToken>,
}

impl CallOptions {
    /// Options with nothing set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Correlate on this exact ID.
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Reject the call after `timeout`.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Reject the call when `cancel` fires.
    pub fn with_cancel(mut self, cancel: CancelToken) -> Self {
        self.cancel = Some(cancel);
        self
    }
}

/// Per-notification knobs. A notification has no ID and never settles.
#[derive(Debug, Clone, Default)]
pub struct NotifyOptions {
    /// Fail the send once this much time has elapsed.
    pub timeout: Option<Duration>,
    /// Fail the send when this token is cancelled.
    pub cancel: Option<CancelToken>,
}

type DiagnosticSink = Arc<dyn Fn(ClientDiagnostic) + Send + Sync>;

/// Client construction knobs.
#[derive(Clone, Default)]
pub struct ClientOptions {
    diagnostics: Option<DiagnosticSink>,
}

impl std::fmt::Debug for ClientOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientOptions")
            .field("diagnostics", &self.diagnostics.is_some())
            .finish()
    }
}

impl ClientOptions {
    /// Options with no diagnostic observer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe diagnostics through a callback. The callback runs on the
    /// receive pump, so it must not block.
    pub fn with_diagnostics(
        mut self,
        sink: impl Fn(ClientDiagnostic) + Send + Sync + 'static,
    ) -> Self {
        self.diagnostics = Some(Arc::new(sink));
        self
    }

    /// Observe diagnostics through an unbounded channel instead of a callback.
    pub fn with_diagnostic_channel(self) -> (Self, mpsc::UnboundedReceiver<ClientDiagnostic>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            self.with_diagnostics(move |diagnostic| {
                let _ = sender.send(diagnostic);
            }),
            receiver,
        )
    }
}

enum Channel {
    Duplex(Arc<dyn DuplexTransport>),
    RequestResponse(Arc<dyn RequestResponseTransport>),
}

#[derive(Copy, Clone)]
enum Scope<'a> {
    /// A document from the duplex receive pump: any pending call may settle.
    Stream,
    /// A document owned by one exchange: only that call may settle, and a
    /// notification exchange (`None`) may settle nothing at all.
    Exchange(Option<&'a Id>),
}

struct State {
    status: ClientStatus,
    terminal: Option<ClientError>,
    pending: HashMap<Id, oneshot::Sender<Result<Value, ClientError>>>,
    next_id: i64,
}

impl State {
    fn is_terminal(&self) -> bool {
        matches!(self.status, ClientStatus::Closed | ClientStatus::Failed)
    }

    fn terminal_error(&self) -> ClientError {
        self.terminal
            .clone()
            .unwrap_or_else(|| ClientError::Closed("client is closed".into()))
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            status: ClientStatus::Idle,
            terminal: None,
            pending: HashMap::new(),
            next_id: 0,
        }
    }
}

struct Inner {
    channel: Channel,
    diagnostics: Option<DiagnosticSink>,
    state: Mutex<State>,
    terminal: CancelToken,
    opened: OnceCell<Result<(), ClientError>>,
    closed: OnceCell<Result<(), ClientError>>,
    pump: Mutex<Option<JoinHandle<()>>>,
}

/// A lifecycle-safe TOON-RPC client over one transport.
pub struct Client {
    inner: Arc<Inner>,
}

impl Client {
    /// Build a client over a duplex transport.
    pub fn duplex(transport: Arc<dyn DuplexTransport>) -> Self {
        Self::with_channel(Channel::Duplex(transport), ClientOptions::new())
    }

    /// Build a client over a duplex transport with explicit options.
    pub fn duplex_with(transport: Arc<dyn DuplexTransport>, options: ClientOptions) -> Self {
        Self::with_channel(Channel::Duplex(transport), options)
    }

    /// Build a client over a request/response transport.
    pub fn request_response(transport: Arc<dyn RequestResponseTransport>) -> Self {
        Self::with_channel(Channel::RequestResponse(transport), ClientOptions::new())
    }

    /// Build a client over a request/response transport with explicit options.
    pub fn request_response_with(
        transport: Arc<dyn RequestResponseTransport>,
        options: ClientOptions,
    ) -> Self {
        Self::with_channel(Channel::RequestResponse(transport), options)
    }

    fn with_channel(channel: Channel, options: ClientOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                channel,
                diagnostics: options.diagnostics,
                state: Mutex::new(State::default()),
                terminal: CancelToken::new(),
                opened: OnceCell::new(),
                closed: OnceCell::new(),
                pump: Mutex::new(None),
            }),
        }
    }

    /// Current lifecycle state.
    pub fn status(&self) -> ClientStatus {
        self.inner.status()
    }

    /// Number of calls awaiting settlement.
    pub fn pending_call_count(&self) -> usize {
        self.inner.lock().pending.len()
    }

    /// Open the transport eagerly. Calls open it on demand otherwise.
    pub async fn start(&self) -> Result<(), ClientError> {
        Inner::ensure_open(&self.inner).await
    }

    /// Issue a request and await its response.
    pub async fn call(&self, method: &str, params: Params) -> Result<Value, ClientError> {
        self.call_with(method, params, CallOptions::new()).await
    }

    /// Issue a request with an explicit ID, deadline, or cancellation token.
    pub async fn call_with(
        &self,
        method: &str,
        params: Params,
        options: CallOptions,
    ) -> Result<Value, ClientError> {
        if options
            .cancel
            .as_ref()
            .is_some_and(CancelToken::is_cancelled)
        {
            return Err(ClientError::Aborted);
        }
        let id = match &options.id {
            Some(id) => id.clone(),
            None => self.inner.allocate_id()?,
        };
        let document = encode_request(method, params, Some(id.clone()))?;
        let receiver = self.inner.register_pending(id.clone())?;
        let guard = PendingGuard {
            inner: &self.inner,
            id,
        };
        self.inner
            .await_call(&guard.id, document, receiver, &options)
            .await
    }

    /// Send a notification: no ID, no response, no pending entry.
    pub async fn notify(&self, method: &str, params: Params) -> Result<(), ClientError> {
        self.notify_with(method, params, NotifyOptions::default())
            .await
    }

    /// Send a notification with an explicit deadline or cancellation token.
    pub async fn notify_with(
        &self,
        method: &str,
        params: Params,
        options: NotifyOptions,
    ) -> Result<(), ClientError> {
        if options
            .cancel
            .as_ref()
            .is_some_and(CancelToken::is_cancelled)
        {
            return Err(ClientError::Aborted);
        }
        let document = encode_request(method, params, None)?;
        let inner = Arc::clone(&self.inner);
        let work = async move {
            Inner::ensure_open(&inner).await?;
            inner.assert_open()?;
            match &inner.channel {
                Channel::Duplex(transport) => transport.send(document).await?,
                Channel::RequestResponse(transport) => {
                    if let Some(response) = transport.request(document).await? {
                        if !response.is_empty() {
                            inner.process_document(&response, Scope::Exchange(None));
                        }
                    }
                }
            }
            Ok(())
        };
        race_operation(work, &self.inner, options.cancel.as_ref(), options.timeout).await
    }

    /// Terminate the client: every pending call is rejected exactly once and
    /// the transport is closed exactly once. Idempotent.
    pub async fn close(&self) -> Result<(), ClientError> {
        self.inner.terminate(
            ClientStatus::Closed,
            ClientError::Closed("closed by the caller".into()),
        );
        let result = self.inner.close_transport().await;
        let pump = self.inner.pump.lock().expect("client pump lock").take();
        if let Some(pump) = pump {
            let _ = pump.await;
        }
        result
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // The handle is gone, so no further call can be made: settle whatever
        // is still pending and stop the detached pump instead of leaking it.
        self.inner.terminate(
            ClientStatus::Closed,
            ClientError::Closed("client was dropped".into()),
        );
        if let Some(pump) = self.inner.pump.lock().expect("client pump lock").take() {
            pump.abort();
        }
    }
}

struct PendingGuard<'a> {
    inner: &'a Arc<Inner>,
    id: Id,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        // A dropped call future (cancelled at the await point) must not leave
        // its slot behind; removing an already-settled ID is a no-op.
        self.inner.take_pending(&self.id);
    }
}

impl Inner {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("client state lock")
    }

    fn status(&self) -> ClientStatus {
        self.lock().status
    }

    fn terminal_error(&self) -> ClientError {
        self.lock().terminal_error()
    }

    fn assert_open(&self) -> Result<(), ClientError> {
        if self.status() == ClientStatus::Open {
            Ok(())
        } else {
            Err(self.terminal_error())
        }
    }

    fn allocate_id(&self) -> Result<Id, ClientError> {
        let mut state = self.lock();
        while state.pending.contains_key(&Id::Number(state.next_id)) {
            state.next_id += 1;
        }
        if state.next_id > MAX_SAFE_ID {
            return Err(ClientError::InvalidRequest(
                "numeric ID space exhausted".into(),
            ));
        }
        let id = Id::Number(state.next_id);
        state.next_id += 1;
        Ok(id)
    }

    fn register_pending(
        &self,
        id: Id,
    ) -> Result<oneshot::Receiver<Result<Value, ClientError>>, ClientError> {
        let mut state = self.lock();
        if state.is_terminal() {
            return Err(state.terminal_error());
        }
        if state.pending.contains_key(&id) {
            return Err(ClientError::InvalidRequest(format!(
                "call ID is already pending: {}",
                describe_id(&id)
            )));
        }
        let (sender, receiver) = oneshot::channel();
        state.pending.insert(id, sender);
        Ok(receiver)
    }

    fn take_pending(&self, id: &Id) -> Option<oneshot::Sender<Result<Value, ClientError>>> {
        self.lock().pending.remove(id)
    }

    fn is_pending(&self, id: &Id) -> bool {
        self.lock().pending.contains_key(id)
    }

    fn diagnostic(&self, reason: DiagnosticReason, index: Option<usize>, id: Option<Id>) {
        if let Some(sink) = &self.diagnostics {
            sink(ClientDiagnostic { reason, index, id });
        }
    }

    async fn ensure_open(inner: &Arc<Inner>) -> Result<(), ClientError> {
        match inner.status() {
            ClientStatus::Open => return Ok(()),
            ClientStatus::Closed | ClientStatus::Failed => return Err(inner.terminal_error()),
            _ => {}
        }
        let shared = Arc::clone(inner);
        inner
            .opened
            .get_or_init(|| async move { Inner::open(&shared).await })
            .await
            .clone()
    }

    async fn open(inner: &Arc<Inner>) -> Result<(), ClientError> {
        {
            let mut state = inner.lock();
            if state.is_terminal() {
                return Err(state.terminal_error());
            }
            state.status = ClientStatus::Opening;
        }

        let opening = async {
            match &inner.channel {
                Channel::Duplex(transport) => transport.open().await,
                Channel::RequestResponse(transport) => transport.open().await,
            }
        };
        let outcome = tokio::select! {
            result = opening => result.map_err(ClientError::from),
            () = inner.terminal.cancelled() => Err(inner.terminal_error()),
        };

        if let Err(error) = outcome {
            inner.terminate(ClientStatus::Failed, error.clone());
            let _ = inner.close_transport().await;
            return Err(error);
        }

        {
            let mut state = inner.lock();
            if state.status != ClientStatus::Opening {
                drop(state);
                return Err(inner.terminal_error());
            }
            state.status = ClientStatus::Open;
        }

        if matches!(inner.channel, Channel::Duplex(_)) {
            let pump = Arc::clone(inner);
            *inner.pump.lock().expect("client pump lock") =
                Some(tokio::spawn(async move { Inner::receive_loop(pump).await }));
        }
        Ok(())
    }

    async fn receive_loop(inner: Arc<Inner>) {
        let Channel::Duplex(transport) = &inner.channel else {
            return;
        };
        loop {
            let next = tokio::select! {
                () = inner.terminal.cancelled() => return,
                result = transport.receive() => result,
            };
            if inner.status() != ClientStatus::Open {
                return;
            }
            match next {
                Ok(Some(document)) => inner.process_document(&document, Scope::Stream),
                Ok(None) => {
                    inner.terminate(
                        ClientStatus::Closed,
                        ClientError::Closed("transport closed".into()),
                    );
                    let _ = inner.close_transport().await;
                    return;
                }
                Err(error) => {
                    inner.terminate(ClientStatus::Failed, ClientError::Transport(error));
                    let _ = inner.close_transport().await;
                    return;
                }
            }
        }
    }

    async fn await_call(
        self: &Arc<Self>,
        id: &Id,
        document: Vec<u8>,
        receiver: oneshot::Receiver<Result<Value, ClientError>>,
        options: &CallOptions,
    ) -> Result<Value, ClientError> {
        let inner = Arc::clone(self);
        let owned_id = id.clone();
        // A successful dispatch does not settle the call: a duplex response
        // arrives on the pump, and terminating the client settles through the
        // pending channel. Only a dispatch failure short-circuits here.
        let dispatch = async move {
            match inner.dispatch(&owned_id, document).await {
                Ok(()) => std::future::pending::<ClientError>().await,
                Err(error) => error,
            }
        };
        let settled = async move {
            receiver
                .await
                .unwrap_or_else(|_| Err(ClientError::Closed("call was dropped".into())))
        };

        let cancelled = wait_for_cancel(options.cancel.as_ref());
        let deadline = wait_for_deadline(options.timeout);
        tokio::pin!(dispatch, settled, cancelled, deadline);

        tokio::select! {
            biased;
            outcome = &mut settled => outcome,
            error = &mut dispatch => Err(error),
            () = &mut cancelled => Err(ClientError::Aborted),
            () = &mut deadline => Err(ClientError::Timeout(
                options.timeout.unwrap_or_default().as_millis(),
            )),
        }
    }

    async fn dispatch(self: &Arc<Self>, id: &Id, document: Vec<u8>) -> Result<(), ClientError> {
        Inner::ensure_open(self).await?;
        if !self.is_pending(id) {
            return Ok(());
        }
        self.assert_open()?;
        match &self.channel {
            Channel::Duplex(transport) => {
                transport.send(document).await?;
                Ok(())
            }
            Channel::RequestResponse(transport) => {
                let response = transport.request(document).await?;
                if !self.is_pending(id) {
                    return Ok(());
                }
                let Some(response) = response.filter(|document| !document.is_empty()) else {
                    return Err(ClientError::Protocol(
                        "request/response transport returned no response".into(),
                    ));
                };
                self.process_document(&response, Scope::Exchange(Some(id)));
                if self.is_pending(id) {
                    // The exchange is exhausted (spec section 8.2): this call
                    // can never be matched by a later document.
                    return Err(ClientError::Protocol(
                        "request/response document did not contain the matching response".into(),
                    ));
                }
                Ok(())
            }
        }
    }

    fn process_document(&self, document: &[u8], scope: Scope<'_>) {
        let Ok(value) = decode_wire_value(document) else {
            self.diagnostic(DiagnosticReason::ParseError, None, None);
            return;
        };

        let ToonValue::Array(ToonArray::List(entries)) = &value else {
            match response_from_value(&value) {
                Ok(response) => {
                    self.settle(response, None, scope);
                }
                Err(_) => self.diagnostic(DiagnosticReason::InvalidResponse, None, None),
            }
            return;
        };

        if entries.is_empty() {
            self.diagnostic(DiagnosticReason::InvalidResponse, None, None);
            return;
        }
        let mut settled = HashSet::new();
        for (index, entry) in entries.iter().enumerate() {
            let Ok(response) = response_from_value(entry) else {
                self.diagnostic(DiagnosticReason::InvalidResponse, Some(index), None);
                continue;
            };
            if settled.contains(&response.id) {
                self.diagnostic(
                    DiagnosticReason::DuplicateId,
                    Some(index),
                    Some(response.id.clone()),
                );
                continue;
            }
            let id = response.id.clone();
            if self.settle(response, Some(index), scope) {
                settled.insert(id);
            }
        }
    }

    fn settle(&self, response: Response, index: Option<usize>, scope: Scope<'_>) -> bool {
        if let Scope::Exchange(expected) = scope {
            if expected != Some(&response.id) {
                self.diagnostic(DiagnosticReason::UnknownId, index, Some(response.id));
                return false;
            }
        }
        let Some(pending) = self.take_pending(&response.id) else {
            self.diagnostic(DiagnosticReason::UnknownId, index, Some(response.id));
            return false;
        };
        let outcome = match (response.result, response.error) {
            (Some(result), None) => Ok(result),
            (None, Some(error)) => Err(ClientError::Rpc(error)),
            // The response decoder guarantees exactly one member is present.
            _ => Err(ClientError::Protocol(
                "response must contain exactly one of result and error".into(),
            )),
        };
        let _ = pending.send(outcome);
        true
    }

    fn terminate(&self, status: ClientStatus, error: ClientError) {
        let pending = {
            let mut state = self.lock();
            if state.is_terminal() {
                return;
            }
            state.status = status;
            state.terminal = Some(error.clone());
            std::mem::take(&mut state.pending)
        };
        self.terminal.cancel();
        for (_, sender) in pending {
            let _ = sender.send(Err(error.clone()));
        }
    }

    async fn close_transport(&self) -> Result<(), ClientError> {
        self.closed
            .get_or_init(|| async {
                match &self.channel {
                    Channel::Duplex(transport) => transport.close().await,
                    Channel::RequestResponse(transport) => transport.close().await,
                }
                .map_err(ClientError::from)
            })
            .await
            .clone()
    }
}

async fn race_operation<F>(
    work: F,
    inner: &Arc<Inner>,
    cancel: Option<&CancelToken>,
    timeout: Option<Duration>,
) -> Result<(), ClientError>
where
    F: std::future::Future<Output = Result<(), ClientError>>,
{
    let cancelled = wait_for_cancel(cancel);
    let deadline = wait_for_deadline(timeout);
    let terminal = inner.terminal.clone();
    tokio::pin!(work, cancelled, deadline);

    tokio::select! {
        biased;
        result = &mut work => result,
        () = &mut cancelled => Err(ClientError::Aborted),
        () = terminal.cancelled() => Err(inner.terminal_error()),
        () = &mut deadline => Err(ClientError::Timeout(
            timeout.unwrap_or_default().as_millis(),
        )),
    }
}

async fn wait_for_cancel(cancel: Option<&CancelToken>) {
    match cancel {
        Some(cancel) => cancel.cancelled().await,
        None => std::future::pending().await,
    }
}

async fn wait_for_deadline(timeout: Option<Duration>) {
    match timeout {
        Some(timeout) => tokio::time::sleep(timeout).await,
        None => std::future::pending().await,
    }
}

fn encode_request(method: &str, params: Params, id: Option<Id>) -> Result<Vec<u8>, ClientError> {
    let call = match id {
        Some(id) => Call::Request(Request::new(method.to_string(), params, id)),
        None => Call::Notification(Notification::new(method.to_string(), params)),
    };
    to_wire(&Message::Single(call)).map_err(|error| ClientError::InvalidRequest(error.to_string()))
}

fn describe_id(id: &Id) -> String {
    match id {
        Id::Null => "null".to_string(),
        Id::String(value) => format!("{value:?}"),
        Id::Number(value) => value.to_string(),
    }
}
