//! Lifecycle, correlation and settlement tests for the production client.
//!
//! Parity target: `packages/toon-rpc/src/client.ts` and its suite.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use reddb_io_toon::Value as ToonValue;
use reddb_io_toon_rpc::client::{
    CallOptions, Client, ClientDiagnostic, ClientError, ClientOptions, ClientStatus,
    DiagnosticReason, NotifyOptions,
};
use reddb_io_toon_rpc::transport::{DuplexTransport, RequestResponseTransport, TransportError};
use reddb_io_toon_rpc::{CancelToken, Id, Params};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

/// Encode a fixture document with the canonical encoder; no hand-written TOON.
fn document(value: Value) -> Vec<u8> {
    reddb_io_toon::encode(&ToonValue::from_json_value(value))
        .expect("fixture must encode")
        .into_bytes()
}

fn success(id: Value, result: Value) -> Value {
    json!({ "toonrpc": "1.0", "result": result, "id": id })
}

enum Event {
    Document(Vec<u8>),
    Failure(String),
    End,
}

#[derive(Default)]
struct MockDuplex {
    sent: Mutex<Vec<Vec<u8>>>,
    inbox: Option<AsyncMutex<mpsc::UnboundedReceiver<Event>>>,
    outbox: Option<mpsc::UnboundedSender<Event>>,
    send_failure: Option<String>,
    open_failure: Option<String>,
    closes: AtomicUsize,
}

impl MockDuplex {
    fn new() -> Arc<Self> {
        let (outbox, inbox) = mpsc::unbounded_channel();
        Arc::new(Self {
            inbox: Some(AsyncMutex::new(inbox)),
            outbox: Some(outbox),
            ..Self::default()
        })
    }

    fn failing_send(message: &str) -> Arc<Self> {
        let mut transport = Arc::into_inner(Self::new()).expect("unique");
        transport.send_failure = Some(message.to_string());
        Arc::new(transport)
    }

    fn failing_open(message: &str) -> Arc<Self> {
        let mut transport = Arc::into_inner(Self::new()).expect("unique");
        transport.open_failure = Some(message.to_string());
        Arc::new(transport)
    }

    fn push(&self, event: Event) {
        let _ = self.outbox.as_ref().expect("outbox").send(event);
    }

    fn sent_count(&self) -> usize {
        self.sent.lock().expect("sent lock").len()
    }

    fn sent(&self, index: usize) -> Value {
        let sent = self.sent.lock().expect("sent lock");
        let text = std::str::from_utf8(&sent[index]).expect("UTF-8");
        reddb_io_toon::decode(text)
            .expect("decodable")
            .to_json_value()
    }

    fn close_count(&self) -> usize {
        self.closes.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DuplexTransport for MockDuplex {
    async fn open(&self) -> Result<(), TransportError> {
        match &self.open_failure {
            Some(message) => Err(TransportError::new(message.clone())),
            None => Ok(()),
        }
    }

    async fn send(&self, document: Vec<u8>) -> Result<(), TransportError> {
        if let Some(message) = &self.send_failure {
            return Err(TransportError::new(message.clone()));
        }
        self.sent.lock().expect("sent lock").push(document);
        Ok(())
    }

    async fn receive(&self) -> Result<Option<Vec<u8>>, TransportError> {
        let mut inbox = self.inbox.as_ref().expect("inbox").lock().await;
        match inbox.recv().await {
            Some(Event::Document(document)) => Ok(Some(document)),
            Some(Event::Failure(message)) => Err(TransportError::new(message)),
            Some(Event::End) | None => Ok(None),
        }
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        self.push(Event::End);
        Ok(())
    }
}

struct MockExchange {
    responses: Mutex<Vec<Option<Vec<u8>>>>,
    requests: Mutex<Vec<Vec<u8>>>,
}

impl MockExchange {
    fn new(responses: Vec<Option<Vec<u8>>>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl RequestResponseTransport for MockExchange {
    async fn request(&self, document: Vec<u8>) -> Result<Option<Vec<u8>>, TransportError> {
        self.requests.lock().expect("requests lock").push(document);
        let mut responses = self.responses.lock().expect("responses lock");
        if responses.is_empty() {
            return Ok(None);
        }
        Ok(responses.remove(0))
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Diagnostics {
    entries: Arc<Mutex<Vec<ClientDiagnostic>>>,
}

impl Diagnostics {
    fn options(&self) -> ClientOptions {
        let entries = Arc::clone(&self.entries);
        ClientOptions::new().with_diagnostics(move |diagnostic| {
            entries.lock().expect("diagnostics lock").push(diagnostic);
        })
    }

    fn reasons(&self) -> Vec<(DiagnosticReason, Option<usize>)> {
        self.entries
            .lock()
            .expect("diagnostics lock")
            .iter()
            .map(|entry| (entry.reason, entry.index))
            .collect()
    }

    fn len(&self) -> usize {
        self.entries.lock().expect("diagnostics lock").len()
    }
}

/// Poll a condition without blocking the runtime; the client settles on its
/// own tasks, so tests observe state instead of sleeping for a fixed span.
async fn wait_for(mut condition: impl FnMut() -> bool) {
    for _ in 0..2000 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("client did not reach the expected state");
}

#[tokio::test]
async fn concurrent_calls_settle_by_typed_id_in_any_order() {
    let transport = MockDuplex::new();
    let client = Arc::new(Client::duplex(transport.clone()));

    let first = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            client
                .call_with(
                    "sum",
                    Params::ByPosition(vec![json!(1)]),
                    CallOptions::new().with_id(Id::Number(1)),
                )
                .await
        }
    });
    let second = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            client
                .call_with(
                    "sum",
                    Params::Absent,
                    CallOptions::new().with_id(Id::String("1".into())),
                )
                .await
        }
    });

    wait_for(|| transport.sent_count() == 2).await;
    assert_eq!(client.status(), ClientStatus::Open);
    assert_eq!(client.pending_call_count(), 2);

    // The string ID answers first: correlation is by typed ID, not arrival.
    transport.push(Event::Document(document(success(json!("1"), json!("b")))));
    transport.push(Event::Document(document(success(json!(1), json!("a")))));

    assert_eq!(second.await.expect("join").expect("settled"), json!("b"));
    assert_eq!(first.await.expect("join").expect("settled"), json!("a"));
    assert_eq!(client.pending_call_count(), 0);
}

#[tokio::test]
async fn a_request_carries_the_protocol_envelope_and_a_notification_has_no_id() {
    let transport = MockDuplex::new();
    let client = Client::duplex(transport.clone());

    client
        .notify("log", Params::ByPosition(vec![json!("hello")]))
        .await
        .expect("notification sent");
    assert_eq!(
        transport.sent(0),
        json!({ "toonrpc": "1.0", "method": "log", "params": ["hello"] })
    );

    let call = tokio::spawn({
        let transport = transport.clone();
        async move {
            wait_for(|| transport.sent_count() == 2).await;
            transport.push(Event::Document(document(success(json!(0), json!(true)))));
        }
    });
    assert_eq!(
        client.call("ping", Params::Absent).await.expect("settled"),
        json!(true)
    );
    call.await.expect("join");
    assert_eq!(
        transport.sent(1),
        json!({ "toonrpc": "1.0", "method": "ping", "id": 0 })
    );
}

#[tokio::test]
async fn an_unknown_id_is_a_diagnostic_and_leaves_the_call_pending() {
    let transport = MockDuplex::new();
    let diagnostics = Diagnostics::default();
    let client = Arc::new(Client::duplex_with(
        transport.clone(),
        diagnostics.options(),
    ));

    let call = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            client
                .call_with("ping", Params::Absent, CallOptions::new().with_id(Id::Null))
                .await
        }
    });
    wait_for(|| transport.sent_count() == 1).await;

    transport.push(Event::Document(document(success(json!(7), json!(1)))));
    wait_for(|| diagnostics.len() == 1).await;
    assert_eq!(
        diagnostics.reasons(),
        [(DiagnosticReason::UnknownId, None)],
        "an unmatched response must not settle another call"
    );
    assert_eq!(client.pending_call_count(), 1);

    transport.push(Event::Document(document(success(
        Value::Null,
        json!("done"),
    ))));
    assert_eq!(call.await.expect("join").expect("settled"), json!("done"));
}

#[tokio::test]
async fn an_undecodable_or_invalid_document_is_a_diagnostic() {
    let transport = MockDuplex::new();
    let diagnostics = Diagnostics::default();
    let client = Arc::new(Client::duplex_with(
        transport.clone(),
        diagnostics.options(),
    ));
    client.start().await.expect("started");

    transport.push(Event::Document(vec![0x22]));
    transport.push(Event::Document(document(json!({
        "toonrpc": "1.0",
        "result": 1,
        "error": { "code": -32000, "message": "both" },
        "id": 1
    }))));
    transport.push(Event::Document(document(json!([]))));

    wait_for(|| diagnostics.len() == 3).await;
    assert_eq!(
        diagnostics.reasons(),
        [
            (DiagnosticReason::ParseError, None),
            (DiagnosticReason::InvalidResponse, None),
            (DiagnosticReason::InvalidResponse, None),
        ]
    );
}

#[tokio::test]
async fn a_batch_settles_valid_siblings_and_rejects_the_rest_per_entry() {
    let transport = MockDuplex::new();
    let diagnostics = Diagnostics::default();
    let client = Arc::new(Client::duplex_with(
        transport.clone(),
        diagnostics.options(),
    ));

    let mut calls = Vec::new();
    for id in [1, 2] {
        let client = Arc::clone(&client);
        calls.push(tokio::spawn(async move {
            client
                .call_with(
                    "ping",
                    Params::Absent,
                    CallOptions::new().with_id(Id::Number(id)),
                )
                .await
        }));
    }
    wait_for(|| client.pending_call_count() == 2 && transport.sent_count() == 2).await;

    transport.push(Event::Document(document(json!([
        success(json!(1), json!("first")),
        { "toonrpc": "1.0", "id": 2 },
        success(json!(1), json!("duplicate")),
        success(json!(9), json!("unknown")),
    ]))));

    assert_eq!(
        calls.remove(0).await.expect("join").expect("settled"),
        json!("first")
    );
    wait_for(|| diagnostics.len() == 3).await;
    assert_eq!(
        diagnostics.reasons(),
        [
            (DiagnosticReason::InvalidResponse, Some(1)),
            (DiagnosticReason::DuplicateId, Some(2)),
            (DiagnosticReason::UnknownId, Some(3)),
        ]
    );
    assert_eq!(client.pending_call_count(), 1, "call 2 stays pending");
    calls.remove(0).abort();
}

#[tokio::test]
async fn an_rpc_error_response_settles_the_call_as_an_error() {
    let transport = MockDuplex::new();
    let client = Arc::new(Client::duplex(transport.clone()));
    let call = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            client
                .call_with(
                    "boom",
                    Params::Absent,
                    CallOptions::new().with_id(Id::Number(4)),
                )
                .await
        }
    });
    wait_for(|| transport.sent_count() == 1).await;

    transport.push(Event::Document(document(json!({
        "toonrpc": "1.0",
        "error": { "code": -32601, "message": "Method not found", "data": [1] },
        "id": 4
    }))));

    let ClientError::Rpc(error) = call.await.expect("join").expect_err("rpc error") else {
        panic!("expected an RPC error object");
    };
    assert_eq!(error.code.code(), -32601);
    assert_eq!(error.message, "Method not found");
    assert_eq!(error.data, Some(json!([1])));
}

#[tokio::test]
async fn a_timeout_rejects_the_call_and_frees_its_slot() {
    let transport = MockDuplex::new();
    let client = Client::duplex(transport.clone());

    let error = client
        .call_with(
            "slow",
            Params::Absent,
            CallOptions::new()
                .with_id(Id::Number(3))
                .with_timeout(Duration::from_millis(10)),
        )
        .await
        .expect_err("timed out");
    assert!(matches!(error, ClientError::Timeout(10)), "{error:?}");
    assert_eq!(client.pending_call_count(), 0);
    assert_eq!(client.status(), ClientStatus::Open, "a timeout is per call");
}

#[tokio::test]
async fn a_cancelled_call_rejects_and_frees_its_slot() {
    let transport = MockDuplex::new();
    let client = Arc::new(Client::duplex(transport.clone()));
    let cancel = CancelToken::new();

    let call = tokio::spawn({
        let client = Arc::clone(&client);
        let cancel = cancel.clone();
        async move {
            client
                .call_with(
                    "slow",
                    Params::Absent,
                    CallOptions::new()
                        .with_id(Id::Number(1))
                        .with_cancel(cancel),
                )
                .await
        }
    });
    wait_for(|| client.pending_call_count() == 1).await;
    cancel.cancel();

    assert!(matches!(
        call.await.expect("join").expect_err("aborted"),
        ClientError::Aborted
    ));
    wait_for(|| client.pending_call_count() == 0).await;

    // A token cancelled up front aborts before anything is dispatched.
    let cancelled = CancelToken::new();
    cancelled.cancel();
    let error = client
        .call_with(
            "slow",
            Params::Absent,
            CallOptions::new().with_cancel(cancelled),
        )
        .await
        .expect_err("aborted");
    assert!(matches!(error, ClientError::Aborted));
    assert_eq!(transport.sent_count(), 1);
}

#[tokio::test]
async fn close_settles_every_pending_call_exactly_once() {
    let transport = MockDuplex::new();
    let client = Arc::new(Client::duplex(transport.clone()));

    let mut calls = Vec::new();
    for id in 0..3 {
        let client = Arc::clone(&client);
        calls.push(tokio::spawn(async move {
            client
                .call_with(
                    "ping",
                    Params::Absent,
                    CallOptions::new().with_id(Id::Number(id)),
                )
                .await
        }));
    }
    wait_for(|| client.pending_call_count() == 3).await;

    client.close().await.expect("closed");
    for call in calls {
        assert!(matches!(
            call.await.expect("join").expect_err("closed"),
            ClientError::Closed(_)
        ));
    }
    assert_eq!(client.pending_call_count(), 0);
    assert_eq!(client.status(), ClientStatus::Closed);

    // Closing twice closes the transport once and still reports success.
    client.close().await.expect("closed again");
    assert_eq!(transport.close_count(), 1);

    let error = client
        .call("ping", Params::Absent)
        .await
        .expect_err("client is closed");
    assert!(matches!(error, ClientError::Closed(_)));
    assert!(matches!(
        client
            .notify("ping", Params::Absent)
            .await
            .expect_err("client is closed"),
        ClientError::Closed(_)
    ));
}

#[tokio::test]
async fn a_transport_failure_terminates_the_client_and_rejects_pending_calls() {
    let transport = MockDuplex::new();
    let client = Arc::new(Client::duplex(transport.clone()));
    let call = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.call("ping", Params::Absent).await }
    });
    wait_for(|| client.pending_call_count() == 1).await;

    transport.push(Event::Failure("connection reset".into()));
    assert!(matches!(
        call.await.expect("join").expect_err("transport failed"),
        ClientError::Transport(_)
    ));
    wait_for(|| client.status() == ClientStatus::Failed).await;
    assert_eq!(client.pending_call_count(), 0);
}

#[tokio::test]
async fn a_completed_receive_stream_closes_the_client() {
    let transport = MockDuplex::new();
    let client = Arc::new(Client::duplex(transport.clone()));
    let call = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.call("ping", Params::Absent).await }
    });
    wait_for(|| client.pending_call_count() == 1).await;

    transport.push(Event::End);
    assert!(matches!(
        call.await.expect("join").expect_err("stream ended"),
        ClientError::Closed(_)
    ));
    wait_for(|| client.status() == ClientStatus::Closed).await;
}

#[tokio::test]
async fn a_send_failure_rejects_only_its_own_call() {
    let transport = MockDuplex::failing_send("broken pipe");
    let client = Client::duplex(transport);

    let error = client
        .call("ping", Params::Absent)
        .await
        .expect_err("send failed");
    assert!(matches!(error, ClientError::Transport(_)), "{error:?}");
    assert_eq!(client.pending_call_count(), 0);
    assert_eq!(client.status(), ClientStatus::Open);
}

#[tokio::test]
async fn an_open_failure_fails_the_client() {
    let transport = MockDuplex::failing_open("refused");
    let client = Client::duplex(transport);

    assert!(matches!(
        client.start().await.expect_err("open failed"),
        ClientError::Transport(_)
    ));
    assert_eq!(client.status(), ClientStatus::Failed);
    // The failure is remembered rather than retried on every call.
    assert!(matches!(
        client
            .call("ping", Params::Absent)
            .await
            .expect_err("still failed"),
        ClientError::Transport(_)
    ));
}

#[tokio::test]
async fn duplicate_and_invalid_call_ids_are_rejected_before_dispatch() {
    let transport = MockDuplex::new();
    let client = Arc::new(Client::duplex(transport.clone()));
    let call = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            client
                .call_with(
                    "ping",
                    Params::Absent,
                    CallOptions::new().with_id(Id::Number(1)),
                )
                .await
        }
    });
    wait_for(|| client.pending_call_count() == 1).await;

    let error = client
        .call_with(
            "ping",
            Params::Absent,
            CallOptions::new().with_id(Id::Number(1)),
        )
        .await
        .expect_err("duplicate pending ID");
    assert!(matches!(error, ClientError::InvalidRequest(_)), "{error:?}");

    let error = client
        .call("", Params::Absent)
        .await
        .expect_err("empty method");
    assert!(matches!(error, ClientError::InvalidRequest(_)), "{error:?}");
    assert_eq!(client.pending_call_count(), 1);
    call.abort();
}

#[tokio::test]
async fn a_request_response_exchange_owns_its_own_response() {
    let transport = MockExchange::new(vec![Some(document(success(json!(0), json!(42))))]);
    let client = Client::request_response(transport);

    assert_eq!(
        client.call("ping", Params::Absent).await.expect("settled"),
        json!(42)
    );
    assert_eq!(client.pending_call_count(), 0);
}

#[tokio::test]
async fn a_request_response_exchange_without_a_matching_response_is_exhausted() {
    let diagnostics = Diagnostics::default();
    let transport = MockExchange::new(vec![
        None,
        Some(document(success(json!(99), json!("other")))),
    ]);
    let client = Client::request_response_with(transport, diagnostics.options());

    let error = client
        .call("ping", Params::Absent)
        .await
        .expect_err("no response");
    assert!(matches!(error, ClientError::Protocol(_)), "{error:?}");

    let error = client
        .call("ping", Params::Absent)
        .await
        .expect_err("mismatched response");
    assert!(matches!(error, ClientError::Protocol(_)), "{error:?}");
    assert_eq!(
        diagnostics.reasons(),
        [(DiagnosticReason::UnknownId, None)],
        "a foreign response must never settle this exchange"
    );
    assert_eq!(client.pending_call_count(), 0);
}

#[tokio::test]
async fn a_request_response_notification_settles_nothing() {
    let diagnostics = Diagnostics::default();
    let transport = MockExchange::new(vec![Some(document(success(json!(0), json!(1))))]);
    let client = Client::request_response_with(transport, diagnostics.options());

    client
        .notify_with("log", Params::Absent, NotifyOptions::default())
        .await
        .expect("notification sent");
    assert_eq!(diagnostics.reasons(), [(DiagnosticReason::UnknownId, None)]);
}

#[tokio::test]
async fn diagnostics_can_be_observed_on_a_channel() {
    let (options, mut receiver) = ClientOptions::new().with_diagnostic_channel();
    let transport = MockDuplex::new();
    let client = Client::duplex_with(transport.clone(), options);
    client.start().await.expect("started");

    transport.push(Event::Document(document(success(json!(5), json!(1)))));
    let diagnostic = receiver.recv().await.expect("diagnostic");
    assert_eq!(diagnostic.reason, DiagnosticReason::UnknownId);
    assert_eq!(diagnostic.id, Some(Id::Number(5)));
    assert_eq!(diagnostic.index, None);
}

#[tokio::test]
async fn allocated_ids_skip_ids_already_pending() {
    let transport = MockDuplex::new();
    let client = Arc::new(Client::duplex(transport.clone()));
    let held = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            client
                .call_with(
                    "ping",
                    Params::Absent,
                    CallOptions::new().with_id(Id::Number(0)),
                )
                .await
        }
    });
    wait_for(|| client.pending_call_count() == 1).await;

    let allocated = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.call("ping", Params::Absent).await }
    });
    wait_for(|| transport.sent_count() == 2).await;
    assert_eq!(transport.sent(1)["id"], json!(1));

    held.abort();
    allocated.abort();
}
