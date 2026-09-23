use super::*;
use reddb_io_toon_rpc::{Client, ClientOptions, Params};
use serde_json::json;
use std::time::Duration;

async fn server() -> (Uri, SseService) {
    let mut dispatcher = Dispatcher::new();
    dispatcher.register("echo", |params, _id| match params {
        Params::ByPosition(mut values) if !values.is_empty() => Ok(values.remove(0)),
        _ => Ok(json!(null)),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}/rpc", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let service = SseService::new(dispatcher);
    tokio::spawn(SseServer::from_listener(listener, service.clone()).serve());
    (uri, service)
}

async fn request(method: Method, uri: String, body: &'static str) -> Response<Incoming> {
    let client = HyperClient::builder(TokioExecutor::new()).build_http();
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Full::new(Bytes::from(body)))
        .unwrap();
    client.request(request).await.unwrap()
}

async fn eventually(mut ready: impl FnMut() -> bool) {
    for _ in 0..200 {
        if ready() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not reached");
}

#[tokio::test]
async fn calls_are_answered_on_the_event_stream() {
    let (uri, service) = server().await;
    let client = Client::duplex(
        SseTransport::connect(uri).await.unwrap(),
        ClientOptions::default(),
    );
    eventually(|| service.session_count() == 1).await;
    let text = "multi\n\nline";
    let calls = (0..8)
        .map(|n| {
            tokio::spawn(client.call(
                "echo",
                Params::ByPosition(vec![json!(format!("{text} {n}"))]),
            ))
        })
        .collect::<Vec<_>>();
    for (n, call) in calls.into_iter().enumerate() {
        assert_eq!(call.await.unwrap().unwrap(), json!(format!("{text} {n}")));
    }
    client.notify("echo", Params::Absent).await.unwrap();
    client.close().await.unwrap();
    eventually(|| service.session_count() == 0).await;
}

#[tokio::test]
async fn a_post_is_only_acknowledged() {
    let (uri, _service) = server().await;
    let stream = request(Method::GET, format!("{uri}?session=s1"), "").await;
    assert_eq!(stream.status(), StatusCode::OK);
    assert_eq!(
        stream.headers()[header::CONTENT_TYPE],
        EVENT_STREAM_CONTENT_TYPE
    );
    let post = request(
        Method::POST,
        format!("{uri}?session=s1"),
        "toonrpc: \"1.0\"\nmethod: echo\nparams[1]: hi\nid: 7",
    )
    .await;
    assert_eq!(post.status(), StatusCode::ACCEPTED);
    assert!(post
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .is_empty());

    let mut reader = EventReader {
        body: Some(stream.into_body()),
        parser: EventParser::new(DEFAULT_MAX_BODY_BYTES),
    };
    let event = reader.next_event().await.unwrap().unwrap();
    let response = reddb_io_toon_rpc::response_from_wire(&event).unwrap();
    assert_eq!(response.result, Some(json!("hi")));
    assert_eq!(response.id, reddb_io_toon_rpc::Id::Number(7));
}

#[tokio::test]
async fn sessions_are_required_unique_and_known() {
    let (uri, _service) = server().await;
    assert_eq!(
        request(Method::GET, uri.to_string(), "").await.status(),
        StatusCode::BAD_REQUEST
    );
    let _open = request(Method::GET, format!("{uri}?session=s2"), "").await;
    assert_eq!(
        request(Method::GET, format!("{uri}?session=s2"), "")
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(Method::POST, format!("{uri}?session=other"), "x")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(Method::PUT, format!("{uri}?session=s2"), "")
            .await
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[test]
fn events_round_trip_through_the_parser() {
    let mut parser = EventParser::new(1024);
    let mut stream = b": comment\r\nevent: ignored\r\n".to_vec();
    stream.extend(encode_event(b"a: 1\n\nb: 2"));
    stream.extend(encode_event(b""));
    for chunk in stream.chunks(3) {
        parser.push(chunk).unwrap();
    }
    assert_eq!(parser.next_event(), Some(b"a: 1\n\nb: 2".to_vec()));
    assert_eq!(parser.next_event(), Some(Vec::new()));
    assert_eq!(parser.next_event(), None);
}

#[test]
fn an_event_over_the_limit_fails_the_stream() {
    let mut parser = EventParser::new(8);
    assert!(parser.push(b"data: 0123456789").is_err());
}

#[test]
fn session_ids_differ_and_join_existing_queries() {
    assert_ne!(new_session_id(), new_session_id());
    let uri: Uri = "http://h/rpc?x=1".parse().unwrap();
    assert_eq!(
        with_session(&uri, "s").unwrap(),
        "http://h/rpc?x=1&session=s"
    );
    assert_eq!(
        session_id(&"http://h/rpc?x=1&session=s".parse().unwrap()),
        Some("s".into())
    );
}

#[tokio::test]
async fn shutdown_ends_open_event_streams() {
    let (trigger, signal) = tokio::sync::oneshot::channel::<()>();
    let server = SseServer::bind("127.0.0.1:0", Dispatcher::new())
        .await
        .unwrap();
    let uri: Uri = format!("http://{}/rpc", server.local_addr().unwrap())
        .parse()
        .unwrap();
    let serving = tokio::spawn(server.serve_with_shutdown(async {
        let _ = signal.await;
    }));
    let transport = SseTransport::connect(uri).await.unwrap();
    trigger.send(()).unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(5), transport.recv())
        .await
        .unwrap();
    assert_eq!(ended.unwrap(), None);
    serving.await.unwrap().unwrap();
}
