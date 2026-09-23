//! Client lifecycle: every pending call settles exactly once, whatever ends it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use reddb_io_toon_rpc::{
    CallOptions, Client, ClientDiagnostic, ClientError, ClientOptions, ClientStatus,
    DiagnosticReason, DuplexTransport, Id, Params, RequestResponseTransport, RpcError,
};
use serde_json::json;
use tokio::sync::mpsc;

type Incoming = Result<Option<Vec<u8>>, RpcError>;

/// A duplex transport the test drives by hand.
struct Scripted {
    incoming: tokio::sync::Mutex<mpsc::UnboundedReceiver<Incoming>>,
    sent: mpsc::UnboundedSender<Vec<u8>>,
}

fn scripted() -> (
    Scripted,
    mpsc::UnboundedSender<Incoming>,
    mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let (push, incoming) = mpsc::unbounded_channel();
    let (sent, outgoing) = mpsc::unbounded_channel();
    let transport = Scripted {
        incoming: tokio::sync::Mutex::new(incoming),
        sent,
    };
    (transport, push, outgoing)
}

#[async_trait]
impl DuplexTransport for Scripted {
    async fn send(&self, document: Vec<u8>) -> Result<(), RpcError> {
        self.sent
            .send(document)
            .map_err(|_| RpcError::TransportError("peer gone".into()))
    }

    async fn recv(&self) -> Result<Option<Vec<u8>>, RpcError> {
        match self.incoming.lock().await.recv().await {
            Some(next) => next,
            None => std::future::pending().await,
        }
    }

    async fn close(&self) -> Result<(), RpcError> {
        Ok(())
    }
}

fn collecting() -> (ClientOptions, Arc<Mutex<Vec<ClientDiagnostic>>>) {
    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let sink = diagnostics.clone();
    let options = ClientOptions {
        on_diagnostic: Some(Arc::new(move |diagnostic: &ClientDiagnostic| {
            sink.lock().unwrap().push(diagnostic.clone())
        })),
        ..ClientOptions::default()
    };
    (options, diagnostics)
}

fn response(id: i64, result: &str) -> Vec<u8> {
    format!("toonrpc: \"1.0\"\nresult: {result}\nid: {id}").into_bytes()
}

async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn responses_settle_calls_by_id_in_any_order() {
    let (transport, push, mut sent) = scripted();
    let client = Client::duplex(transport, ClientOptions::default());
    let first = client.call("a", Params::Absent);
    let second = client.call("b", Params::Absent);
    assert_eq!(client.pending_call_count(), 2);
    let (first, second) = (tokio::spawn(first), tokio::spawn(second));
    sent.recv().await.unwrap();
    sent.recv().await.unwrap();
    push.send(Ok(Some(response(1, "two")))).unwrap();
    push.send(Ok(Some(response(0, "one")))).unwrap();
    assert_eq!(first.await.unwrap().unwrap(), json!("one"));
    assert_eq!(second.await.unwrap().unwrap(), json!("two"));
    assert_eq!(client.pending_call_count(), 0);
}

#[tokio::test]
async fn a_timeout_or_a_dropped_call_leaves_nothing_pending() {
    let (transport, push, _sent) = scripted();
    let (options, diagnostics) = collecting();
    let client = Client::duplex(transport, options);
    let timed = client.call_with(
        "slow",
        Params::Absent,
        CallOptions {
            id: None,
            timeout: Some(Duration::from_millis(10)),
        },
    );
    assert_eq!(
        timed.await.unwrap_err(),
        ClientError::Timeout(Duration::from_millis(10))
    );
    drop(client.call("dropped", Params::Absent));
    assert_eq!(client.pending_call_count(), 0);

    // A late answer to either call is only a diagnostic.
    push.send(Ok(Some(response(0, "late")))).unwrap();
    settle().await;
    let diagnostics = diagnostics.lock().unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].reason, DiagnosticReason::UnknownId);
    assert_eq!(diagnostics[0].id, Some(Id::Number(0)));
}

#[tokio::test]
async fn an_explicit_id_cannot_be_pending_twice() {
    let (transport, _push, _sent) = scripted();
    let client = Client::duplex(transport, ClientOptions::default());
    let options = CallOptions {
        id: Some(Id::String("same".into())),
        timeout: None,
    };
    let _first = client.call_with("a", Params::Absent, options.clone());
    let second = client.call_with("b", Params::Absent, options);
    assert!(matches!(second.await, Err(ClientError::InvalidCall(_))));
    assert_eq!(client.pending_call_count(), 1);
}

#[tokio::test]
async fn the_stream_ending_or_failing_rejects_every_pending_call() {
    for (ending, failed) in [
        (Ok(None), false),
        (Err(RpcError::TransportError("reset".into())), true),
    ] {
        let (transport, push, _sent) = scripted();
        let client = Client::duplex(transport, ClientOptions::default());
        let calls = [
            client.call("a", Params::Absent),
            client.call("b", Params::Absent),
        ]
        .map(tokio::spawn);
        push.send(ending).unwrap();
        for call in calls {
            let error = call.await.unwrap().unwrap_err();
            assert!(matches!(
                (&error, failed),
                (ClientError::Closed(_), false) | (ClientError::Transport(_), true)
            ));
        }
        let status = if failed {
            ClientStatus::Failed
        } else {
            ClientStatus::Closed
        };
        assert_eq!(client.status(), status);
        assert!(client.call("after", Params::Absent).await.is_err());
    }
}

#[tokio::test]
async fn close_rejects_pending_calls_and_refuses_new_ones() {
    let (transport, _push, _sent) = scripted();
    let client = Client::duplex(transport, ClientOptions::default());
    let pending = tokio::spawn(client.call("a", Params::Absent));
    settle().await;
    client.close().await.unwrap();
    assert!(matches!(
        pending.await.unwrap(),
        Err(ClientError::Closed(_))
    ));
    assert!(matches!(
        client.notify("n", Params::Absent).await,
        Err(ClientError::Closed(_))
    ));
    assert_eq!(client.pending_call_count(), 0);
}

/// A request/response transport answering from a fixed script.
struct Answering(Mutex<Vec<Option<Vec<u8>>>>);

#[async_trait]
impl RequestResponseTransport for Answering {
    async fn request(&self, _document: Vec<u8>) -> Result<Option<Vec<u8>>, RpcError> {
        Ok(self.0.lock().unwrap().remove(0))
    }
}

#[tokio::test]
async fn a_request_response_call_only_accepts_its_own_response() {
    let (options, diagnostics) = collecting();
    let client = Client::request_response(
        Answering(Mutex::new(vec![
            Some(response(0, "mine")),
            Some(response(7, "someone else's")),
            None,
            Some(response(3, "to a notification")),
        ])),
        options,
    );
    assert_eq!(
        client.call("a", Params::Absent).await.unwrap(),
        json!("mine")
    );
    assert!(matches!(
        client.call("b", Params::Absent).await,
        Err(ClientError::Protocol(_))
    ));
    assert!(matches!(
        client.call("c", Params::Absent).await,
        Err(ClientError::Protocol(_))
    ));
    client.notify("n", Params::Absent).await.unwrap();

    let reasons = diagnostics
        .lock()
        .unwrap()
        .iter()
        .map(|diagnostic| (diagnostic.reason, diagnostic.id.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        [
            (DiagnosticReason::UnknownId, Some(Id::Number(7))),
            (DiagnosticReason::UnknownId, Some(Id::Number(3))),
        ]
    );
    assert_eq!(client.pending_call_count(), 0);
}
