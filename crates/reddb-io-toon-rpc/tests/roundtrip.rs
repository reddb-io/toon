use reddb_io_toon_rpc::protocol::{Call, Message, Request, Response};
use reddb_io_toon_rpc::types::{Id, Params};
use reddb_io_toon_rpc::{from_wire, to_wire};

#[test]
fn test_roundtrip_simple() {
    let request = Request::new(
        "echo".to_string(),
        Params::ByPosition(vec![serde_json::json!("hello")]),
        Id::Number(1),
    );
    let msg = Message::Single(Call::Request(request));
    let wire = to_wire(&msg).expect("to_wire failed");
    println!("Wire: {}", String::from_utf8_lossy(&wire));

    let parsed = from_wire(&wire).expect("from_wire failed");
    println!("Parsed: {:#?}", parsed);

    match parsed {
        Message::Single(Call::Request(_)) => {}
        _ => panic!("Expected Single(Request)"),
    }
}

#[test]
fn test_roundtrip_response() {
    let response = Response::success(serde_json::json!("hello"), Id::Number(1));
    let msg = Message::SingleResponse(response);
    let wire = to_wire(&msg).expect("to_wire failed");
    println!("Wire: {}", String::from_utf8_lossy(&wire));

    let parsed = from_wire(&wire).expect("from_wire failed");
    println!("Parsed: {:#?}", parsed);

    match parsed {
        Message::SingleResponse(_) => {}
        _ => panic!("Expected SingleResponse"),
    }
}
