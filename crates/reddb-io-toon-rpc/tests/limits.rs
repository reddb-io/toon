//! Limits are always visible: a refused batch or call, never a silent drop.

use std::time::Duration;

use reddb_io_toon_rpc::{
    response_from_wire, CallOptions, Client, ClientError, ClientOptions, Dispatcher, ErrorCode,
    FramedTransport, Id, Params,
};
use serde_json::json;

fn batch(length: usize) -> String {
    let entries = (0..length)
        .map(|n| format!("  - toonrpc: \"1.0\"\n    method: m\n    id: {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("[{length}]:\n{entries}")
}

#[test]
fn a_batch_over_the_limit_is_one_invalid_request() {
    let mut dispatcher = Dispatcher::new().with_max_batch_length(2);
    dispatcher.register("m", |_, _| Ok(json!(1)));

    let within = dispatcher.dispatch(batch(2).as_bytes()).unwrap();
    assert!(String::from_utf8(within).unwrap().starts_with("[2]"));

    let over = dispatcher.dispatch(batch(3).as_bytes()).unwrap();
    let response = response_from_wire(&over).unwrap();
    assert_eq!(response.id, Id::Null);
    let error = response.error.unwrap();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(error.message, "Invalid Request: batch too large");
}

#[tokio::test]
async fn the_pending_call_cap_refuses_the_next_call() {
    let (client_end, _server_end) = tokio::io::duplex(1024);
    let (reader, writer) = tokio::io::split(client_end);
    let client = Client::duplex(
        FramedTransport::new(reader, writer),
        ClientOptions {
            max_pending_calls: 2,
            ..ClientOptions::default()
        },
    );
    let _first = client.call("a", Params::Absent);
    let _second = client.call("b", Params::Absent);
    assert!(matches!(
        client.call("c", Params::Absent).await,
        Err(ClientError::Limit(_))
    ));
    drop(_first);
    let _third = client.call("c", Params::Absent);
    assert_eq!(client.pending_call_count(), 2);
}

#[tokio::test]
async fn the_default_request_timeout_applies_to_calls_without_one() {
    let (client_end, _server_end) = tokio::io::duplex(1024);
    let (reader, writer) = tokio::io::split(client_end);
    let client = Client::duplex(
        FramedTransport::new(reader, writer),
        ClientOptions {
            request_timeout: Some(Duration::from_millis(10)),
            ..ClientOptions::default()
        },
    );
    assert_eq!(
        client.call("slow", Params::Absent).await,
        Err(ClientError::Timeout(Duration::from_millis(10)))
    );
    let own = CallOptions {
        id: None,
        timeout: Some(Duration::from_millis(20)),
    };
    assert_eq!(
        client.call_with("slow", Params::Absent, own).await,
        Err(ClientError::Timeout(Duration::from_millis(20)))
    );
    assert_eq!(client.pending_call_count(), 0);
}
