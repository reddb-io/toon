use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use reddb_io_toon_rpc::{
    from_wire, response_from_wire, to_wire, Call, Dispatcher, Error, ErrorCode, Id, Message,
    Params, Request, Response, RpcError,
};
use serde_json::{json, Value};

fn wire(value: Value) -> Vec<u8> {
    reddb_io_toon::encode(&reddb_io_toon::Value::from_json_value(value))
        .unwrap()
        .into_bytes()
}

fn value(raw: &[u8]) -> Value {
    reddb_io_toon::decode(std::str::from_utf8(raw).unwrap())
        .unwrap()
        .to_json_value()
}

fn response(raw: &[u8]) -> Response {
    let Message::SingleResponse(response) = from_wire(raw).unwrap() else {
        panic!("expected one response");
    };
    response
}

fn nested_value(depth: usize) -> Value {
    (0..depth).fold(Value::Null, |value, _| {
        let mut object = serde_json::Map::new();
        object.insert("nested".into(), value);
        Value::Object(object)
    })
}

fn drop_value_iteratively(value: Value) {
    let mut pending = vec![value];
    while let Some(mut value) = pending.pop() {
        match &mut value {
            Value::Array(values) => pending.append(values),
            Value::Object(object) => pending.extend(std::mem::take(object).into_values()),
            _ => {}
        }
    }
}

#[test]
fn syntax_errors_are_distinct_from_invalid_decoded_requests() {
    assert!(matches!(
        from_wire(b"toonrpc: \"unterminated"),
        Err(RpcError::ParseError(_))
    ));
    assert!(matches!(
        from_wire(&[0xff, 0xfe]),
        Err(RpcError::ParseError(_))
    ));
    assert!(matches!(
        from_wire(&wire(json!(1))),
        Ok(Message::Invalid(_))
    ));

    let dispatcher = Dispatcher::new();
    let parse_error = response(&dispatcher.dispatch(b"toonrpc: \"unterminated").unwrap());
    assert_eq!(parse_error.error.unwrap().code, ErrorCode::ParseError);

    let invalid_request = response(&dispatcher.dispatch(&wire(json!(1))).unwrap());
    assert_eq!(
        invalid_request.error.unwrap().code,
        ErrorCode::InvalidRequest
    );
}

#[test]
fn core_numbers_are_validated_recursively_on_decode_and_encode() {
    let unsafe_nested =
        b"toonrpc: \"1.0\"\nmethod: echo\nparams:\n  nested[1]: 9007199254740992\nid: 1";
    assert!(matches!(from_wire(unsafe_nested), Ok(Message::Invalid(_))));

    let overflow = b"toonrpc: \"1.0\"\nmethod: echo\nparams[1]: 1e400\nid: 1";
    assert!(matches!(from_wire(overflow), Ok(Message::Invalid(_))));

    let message = Message::SingleResponse(Response::success(
        json!({"nested": [9_007_199_254_740_992_u64]}),
        Id::Number(1),
    ));
    assert!(matches!(
        to_wire(&message),
        Err(RpcError::SerializationError(_))
    ));
}

#[test]
fn request_presence_distinguishes_null_id_and_omitted_params() {
    let Message::Single(Call::Notification(notification)) =
        from_wire(&wire(json!({"toonrpc": "1.0", "method": "observe"}))).unwrap()
    else {
        panic!("absent id must be a notification");
    };
    assert_eq!(notification.params, Params::Absent);

    let Message::Single(Call::Request(request)) = from_wire(&wire(json!({
        "toonrpc": "1.0",
        "method": "observe",
        "params": [],
        "id": null
    })))
    .unwrap() else {
        panic!("explicit null id must be a request");
    };
    assert_eq!(request.id, Id::Null);
    assert_eq!(request.params, Params::ByPosition(vec![]));

    let Message::Single(Call::Request(request)) = from_wire(&wire(json!({
        "toonrpc": "1.0",
        "method": "observe",
        "params": {},
        "id": 1
    })))
    .unwrap() else {
        panic!("expected named params");
    };
    assert_eq!(request.params, Params::ByName(Default::default()));

    assert!(matches!(
        from_wire(&wire(json!({
            "toonrpc": "1.0",
            "method": "observe",
            "params": null,
            "id": 1
        }))),
        Ok(Message::Single(Call::Invalid(_)))
    ));

    let mut dispatcher = Dispatcher::new();
    dispatcher.register("observe", |_params, _id| Ok(json!(true)));
    let explicit_null = dispatcher
        .dispatch(&wire(json!({
            "toonrpc": "1.0",
            "method": "observe",
            "id": null
        })))
        .unwrap();
    assert_eq!(response(&explicit_null).id, Id::Null);
}

#[test]
fn response_branch_and_error_data_use_member_presence() {
    let null_result = wire(json!({"toonrpc": "1.0", "result": null, "id": 1}));
    let parsed = response(&null_result);
    assert_eq!(parsed.result, Some(Value::Null));
    assert_eq!(parsed.error, None);
    let encoded = value(&to_wire(&Message::SingleResponse(parsed)).unwrap());
    assert_eq!(encoded, json!({"toonrpc": "1.0", "result": null, "id": 1}));

    for invalid in [
        json!({"toonrpc": "1.0", "result": 1, "error": {"code": 1, "message": "bad"}, "id": 1}),
        json!({"toonrpc": "1.0", "id": 1}),
    ] {
        assert!(matches!(from_wire(&wire(invalid)), Ok(Message::Invalid(_))));
    }
    let both = Message::SingleResponse(Response {
        toonrpc: "1.0".into(),
        result: Some(json!(1)),
        error: Some(Error::new(ErrorCode::InternalError)),
        id: Id::Number(1),
    });
    assert!(matches!(
        to_wire(&both),
        Err(RpcError::SerializationError(_))
    ));

    let absent = response(&wire(json!({
        "toonrpc": "1.0",
        "error": {"code": 1000, "message": "absent"},
        "id": 2
    })));
    assert_eq!(absent.error.unwrap().data, None);

    let explicit_null = response(&wire(json!({
        "toonrpc": "1.0",
        "error": {"code": 1001, "message": "null", "data": null},
        "id": 3
    })));
    let encoded = value(&to_wire(&Message::SingleResponse(explicit_null)).unwrap());
    assert_eq!(encoded["error"]["data"], Value::Null);
    assert!(encoded["error"].as_object().unwrap().contains_key("data"));
}

#[test]
fn response_decoder_ignores_unknown_method_and_emits_canonical_members() {
    let parsed = response_from_wire(&wire(json!({
        "toonrpc": "1.0",
        "method": "unknown.response.member",
        "error": {"code": 1000, "message": "failed", "extension": true},
        "id": "call-1"
    })))
    .expect("unknown response members must be ignored");
    assert_eq!(parsed.id, Id::String("call-1".into()));
    assert_eq!(parsed.error.as_ref().unwrap().code.code(), 1000);

    let encoded = value(&to_wire(&Message::SingleResponse(parsed)).unwrap());
    assert_eq!(
        encoded,
        json!({
            "toonrpc": "1.0",
            "error": {"code": 1000, "message": "failed"},
            "id": "call-1"
        })
    );
}

#[test]
fn arbitrary_i32_error_codes_round_trip_exactly() {
    for code in [i32::MIN, -32604, 1000, i32::MAX] {
        let original = Message::SingleResponse(Response::error(
            Error::with_message(
                ErrorCode::from_code(code).expect("all i32 codes are supported"),
                "application failure",
            ),
            Id::Number(1),
        ));
        let encoded = to_wire(&original).unwrap();
        let Message::SingleResponse(decoded) = from_wire(&encoded).unwrap() else {
            panic!("expected error response");
        };
        assert_eq!(decoded.error.unwrap().code.code(), code);
    }
}

#[test]
fn dispatcher_generates_correlated_errors_only_for_valid_requests() {
    let mut dispatcher = Dispatcher::new();
    dispatcher.register("reject", |_params, _id| {
        Err(RpcError::InvalidParams("rejected".into()))
    });
    dispatcher.register("application", |_params, _id| {
        Err(RpcError::ApplicationError(i32::MIN, "minimum".into()))
    });

    let malformed = response(
        &dispatcher
            .dispatch(&wire(
                json!({"toonrpc": "0.9", "method": "reject", "id": 7}),
            ))
            .unwrap(),
    );
    assert_eq!(malformed.id, Id::Null);
    assert_eq!(malformed.error.unwrap().code, ErrorCode::InvalidRequest);

    let missing = response(
        &dispatcher
            .dispatch(&wire(
                json!({"toonrpc": "1.0", "method": "missing", "id": 8}),
            ))
            .unwrap(),
    );
    assert_eq!(missing.id, Id::Number(8));
    assert_eq!(missing.error.unwrap().code, ErrorCode::MethodNotFound);

    let rejected = response(
        &dispatcher
            .dispatch(&wire(
                json!({"toonrpc": "1.0", "method": "reject", "id": 9}),
            ))
            .unwrap(),
    );
    assert_eq!(rejected.id, Id::Number(9));
    assert_eq!(rejected.error.unwrap().code, ErrorCode::InvalidParams);

    let application = response(
        &dispatcher
            .dispatch(&wire(json!({
                "toonrpc": "1.0",
                "method": "application",
                "id": 10
            })))
            .unwrap(),
    );
    assert_eq!(application.error.unwrap().code.code(), i32::MIN);

    let notification = dispatcher
        .dispatch(&wire(json!({"toonrpc": "1.0", "method": "reject"})))
        .unwrap();
    assert!(notification.is_empty());
}

#[test]
fn batch_entries_are_independent_and_root_shape_is_preserved() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut dispatcher = Dispatcher::new();
    dispatcher.register("ok", move |_params, _id| {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(json!(true))
    });

    let batch = wire(json!([
        {"toonrpc": "0.9", "method": "ok", "id": 1},
        {"toonrpc": "1.0", "method": "ok"},
        {"toonrpc": "1.0", "method": "ok", "id": 2}
    ]));
    let output = dispatcher.dispatch(&batch).unwrap();
    let root = value(&output);
    let responses = root
        .as_array()
        .expect("batch response must remain an array");
    assert_eq!(responses.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(responses
        .iter()
        .any(|entry| entry["error"]["code"] == -32600));
    assert!(responses.iter().any(|entry| entry["id"] == 2));

    let single_remaining = dispatcher
        .dispatch(&wire(json!([
            {"toonrpc": "1.0", "method": "ok"},
            {"toonrpc": "1.0", "method": "ok", "id": 3}
        ])))
        .unwrap();
    assert_eq!(value(&single_remaining).as_array().unwrap().len(), 1);

    let all_notifications = dispatcher
        .dispatch(&wire(json!([
            {"toonrpc": "1.0", "method": "ok"},
            {"toonrpc": "1.0", "method": "ok", "params": {}}
        ])))
        .unwrap();
    assert!(all_notifications.is_empty());

    let empty = dispatcher.dispatch(&wire(json!([]))).unwrap();
    assert!(value(&empty).is_object());
    let empty = response(&empty);
    assert_eq!(empty.error.unwrap().code, ErrorCode::InvalidRequest);

    let response_shaped_entries = dispatcher
        .dispatch(&wire(json!([
            {"toonrpc": "1.0", "result": true, "id": 4},
            {"toonrpc": "1.0", "error": {"code": 1000, "message": "bad"}, "id": 5}
        ])))
        .unwrap();
    let response_shaped_entries = value(&response_shaped_entries);
    let response_shaped_entries = response_shaped_entries.as_array().unwrap();
    assert_eq!(response_shaped_entries.len(), 2);
    assert!(response_shaped_entries
        .iter()
        .all(|entry| entry["error"]["code"] == -32600 && entry["id"].is_null()));
}

#[test]
fn batch_core_value_validation_is_entry_local() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut dispatcher = Dispatcher::new();
    dispatcher.register("ok", move |_params, _id| {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(json!(true))
    });

    let output = dispatcher
        .dispatch(&wire(json!([
            {
                "toonrpc": "1.0",
                "method": "ok",
                "params": {"nested": [9_007_199_254_740_992_u64]},
                "id": 1
            },
            {"toonrpc": "1.0", "method": "ok"},
            {"toonrpc": "1.0", "method": "ok", "id": 2}
        ])))
        .unwrap();

    let output = value(&output);
    let responses = output.as_array().expect("batch shape must be preserved");
    assert_eq!(responses.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(responses.iter().any(|entry| {
        entry["error"]["code"] == -32600
            && entry["id"].is_null()
            && entry.as_object().unwrap().len() == 3
    }));
    assert!(responses
        .iter()
        .any(|entry| entry["result"] == true && entry["id"] == 2));
}

#[test]
fn invalid_handler_results_become_correlated_internal_errors() {
    let mut dispatcher = Dispatcher::new();
    dispatcher.register("unsafe", |_params, _id| {
        Ok(json!({"nested": [9_007_199_254_740_992_u64]}))
    });
    dispatcher.register("ok", |_params, _id| Ok(json!(true)));

    let single = response(
        &dispatcher
            .dispatch(&wire(json!({
                "toonrpc": "1.0",
                "method": "unsafe",
                "id": 10
            })))
            .unwrap(),
    );
    assert_eq!(single.id, Id::Number(10));
    assert_eq!(single.error.unwrap().code, ErrorCode::InternalError);

    let batch = dispatcher
        .dispatch(&wire(json!([
            {"toonrpc": "1.0", "method": "unsafe", "id": 11},
            {"toonrpc": "1.0", "method": "ok", "id": 12}
        ])))
        .unwrap();
    let batch = value(&batch);
    let responses = batch.as_array().expect("batch shape must be preserved");
    assert_eq!(responses.len(), 2);
    assert!(responses
        .iter()
        .any(|entry| entry["error"]["code"] == -32603 && entry["id"] == 11));
    assert!(responses
        .iter()
        .any(|entry| entry["result"] == true && entry["id"] == 12));
}

#[test]
fn handler_error_data_presence_is_preserved() {
    let mut dispatcher = Dispatcher::new();
    dispatcher.register("null-data", |_params, _id| {
        Err(Error::with_data(ErrorCode::Other(1001), Value::Null).into())
    });
    dispatcher.register("value-data", |_params, _id| {
        Err(Error::with_data(ErrorCode::Other(1002), json!({"reason": "details"})).into())
    });
    dispatcher.register("invalid-params-data", |_params, _id| {
        Err(Error {
            code: ErrorCode::InvalidParams,
            message: "custom invalid params".into(),
            data: Some(Value::Null),
        }
        .into())
    });
    dispatcher.register("internal-data", |_params, _id| {
        Err(Error {
            code: ErrorCode::InternalError,
            message: "custom internal failure".into(),
            data: Some(json!({"trace": "details"})),
        }
        .into())
    });

    let null_data = dispatcher
        .dispatch(&wire(json!({
            "toonrpc": "1.0",
            "method": "null-data",
            "id": 20
        })))
        .unwrap();
    let null_data = value(&null_data);
    assert!(null_data["error"].as_object().unwrap().contains_key("data"));
    assert_eq!(null_data["error"]["data"], Value::Null);

    let value_data = dispatcher
        .dispatch(&wire(json!({
            "toonrpc": "1.0",
            "method": "value-data",
            "id": 21
        })))
        .unwrap();
    assert_eq!(
        value(&value_data)["error"]["data"],
        json!({"reason": "details"})
    );

    let invalid_params = response(
        &dispatcher
            .dispatch(&wire(json!({
                "toonrpc": "1.0",
                "method": "invalid-params-data",
                "id": 22
            })))
            .unwrap(),
    );
    let invalid_params = invalid_params.error.unwrap();
    assert_eq!(invalid_params.code, ErrorCode::InvalidParams);
    assert_eq!(invalid_params.message, "custom invalid params");
    assert_eq!(invalid_params.data, Some(Value::Null));

    let internal = response(
        &dispatcher
            .dispatch(&wire(json!({
                "toonrpc": "1.0",
                "method": "internal-data",
                "id": 23
            })))
            .unwrap(),
    );
    let internal = internal.error.unwrap();
    assert_eq!(internal.code, ErrorCode::InternalError);
    assert_eq!(internal.message, "custom internal failure");
    assert_eq!(internal.data, Some(json!({"trace": "details"})));
}

#[test]
fn handler_error_code_boundaries_are_enforced() {
    let mut dispatcher = Dispatcher::new();
    for (method, code) in [
        ("app-low", -32769),
        ("app-high", -31999),
        ("app-reserved-low", -32768),
        ("app-reserved-high", -32000),
    ] {
        dispatcher.register(method, move |_params, _id| {
            Err(RpcError::ApplicationError(code, method.into()))
        });
    }
    dispatcher.register("handler-method-not-found", |_params, _id| {
        Err(RpcError::MethodNotFound("registered handler".into()))
    });
    for (method, code) in [
        ("response-parse", ErrorCode::ParseError),
        ("response-invalid-request", ErrorCode::InvalidRequest),
        ("response-method-not-found", ErrorCode::MethodNotFound),
    ] {
        dispatcher.register(method, move |_params, _id| {
            Err(Error::with_data(code, Value::Null).into())
        });
    }
    for (method, offset) in [
        ("server-low", 0),
        ("server-high", 99),
        ("server-negative", -1),
        ("server-overflow", 100),
    ] {
        dispatcher.register(method, move |_params, _id| {
            Err(RpcError::ServerError(offset, method.into()))
        });
    }

    for (id, method, expected) in [
        (30, "app-low", -32769),
        (31, "app-high", -31999),
        (32, "app-reserved-low", -32603),
        (33, "app-reserved-high", -32603),
        (34, "server-low", -32000),
        (35, "server-high", -32099),
        (36, "server-negative", -32603),
        (37, "server-overflow", -32603),
        (38, "handler-method-not-found", -32603),
        (39, "response-parse", -32603),
        (40, "response-invalid-request", -32603),
        (41, "response-method-not-found", -32603),
    ] {
        let output = response(
            &dispatcher
                .dispatch(&wire(json!({
                    "toonrpc": "1.0",
                    "method": method,
                    "id": id
                })))
                .unwrap(),
        );
        assert_eq!(output.id, Id::Number(id));
        assert_eq!(output.error.unwrap().code.code(), expected, "{method}");
    }
}

#[test]
fn deeply_nested_handler_responses_are_isolated_before_batch_encoding() {
    let mut dispatcher = Dispatcher::new();
    dispatcher.register("deep-success", |_params, _id| {
        Ok(nested_value(reddb_io_toon::DEFAULT_MAX_DEPTH + 1))
    });
    dispatcher.register("deep-error", |_params, _id| {
        Err(Error {
            code: ErrorCode::Other(1003),
            message: "too deep".into(),
            data: Some(nested_value(reddb_io_toon::DEFAULT_MAX_DEPTH + 1)),
        }
        .into())
    });
    dispatcher.register("ok", |_params, _id| Ok(json!(true)));

    let mut typed = dispatcher
        .dispatch_message(Message::Single(Call::Request(Request::new(
            "deep-success".into(),
            Params::Absent,
            Id::Number(49),
        ))))
        .unwrap()
        .remove(0);
    assert_eq!(typed.id, Id::Number(49));
    assert!(typed.error.is_none());
    drop_value_iteratively(typed.result.take().expect("typed result must be preserved"));

    for (id, method) in [(50, "deep-success"), (51, "deep-error")] {
        let output = response(
            &dispatcher
                .dispatch(&wire(json!({
                    "toonrpc": "1.0",
                    "method": method,
                    "id": id
                })))
                .unwrap(),
        );
        assert_eq!(output.id, Id::Number(id));
        let error = output.error.unwrap();
        assert_eq!(error.code, ErrorCode::InternalError);
        assert_eq!(error.message, "Internal error");
        assert_eq!(error.data, None);
    }

    let output = dispatcher
        .dispatch(&wire(json!([
            {"toonrpc": "1.0", "method": "deep-success", "id": 52},
            {"toonrpc": "1.0", "method": "deep-error", "id": 53},
            {"toonrpc": "1.0", "method": "ok", "id": 54}
        ])))
        .unwrap();
    let output = value(&output);
    let responses = output.as_array().expect("batch shape must be preserved");
    assert_eq!(responses.len(), 3);
    for id in [52, 53] {
        assert!(responses.iter().any(|entry| {
            entry["id"] == id
                && entry["error"]["code"] == -32603
                && !entry["error"].as_object().unwrap().contains_key("data")
        }));
    }
    assert!(responses
        .iter()
        .any(|entry| entry["id"] == 54 && entry["result"] == true));
}

#[test]
fn error_code_serialization_requires_canonical_variants() {
    for code in [ErrorCode::ServerError(0), ErrorCode::ServerError(99)] {
        let encoded = serde_json::to_value(code).unwrap();
        let decoded: ErrorCode = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, code);
    }
    for code in [ErrorCode::ServerError(-1), ErrorCode::ServerError(100)] {
        assert!(serde_json::to_value(code).is_err());
        assert!(matches!(
            to_wire(&Message::SingleResponse(Response::error(
                Error::new(code),
                Id::Number(1)
            ))),
            Err(RpcError::SerializationError(_))
        ));
    }

    for code in [ErrorCode::Other(-32604), ErrorCode::Other(1000)] {
        let encoded = serde_json::to_value(code).unwrap();
        let decoded: ErrorCode = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, code);
    }
    for code in [-32700, -32600, -32601, -32602, -32603, -32099, -32000] {
        let code = ErrorCode::Other(code);
        assert!(serde_json::to_value(code).is_err());
        assert!(matches!(
            to_wire(&Message::SingleResponse(Response::error(
                Error::new(code),
                Id::Number(1)
            ))),
            Err(RpcError::SerializationError(_))
        ));
    }
}
