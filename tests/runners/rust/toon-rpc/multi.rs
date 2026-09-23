//! The shared mixed-dialect corpus (`tests/corpus/toon-rpc/multi.json`), run
//! against the Rust `MultiRpc`. `packages/multi-rpc/test/corpus.test.mjs`
//! runs the same file against the TypeScript one.

use std::path::Path;

use reddb_io_toon_rpc::{Dispatcher, MultiRpc, Params, Protocol, RpcError};
use serde_json::{json, Value};

const CASE_COUNT: usize = 19;

fn multi() -> MultiRpc {
    let mut dispatcher = Dispatcher::new();
    dispatcher.register("echo", |params, _id| match params {
        Params::ByPosition(values) => Ok(values.into_iter().next().unwrap_or(Value::Null)),
        _ => Ok(Value::Null),
    });
    dispatcher.register("fail", |_params, _id| {
        Err(RpcError::ApplicationError(1, "failed".into()))
    });
    MultiRpc::new(dispatcher)
}

/// Exact match, except an error message is only compared when expected.
fn assert_matches(actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Array(expected) => {
            let actual = actual
                .as_array()
                .unwrap_or_else(|| panic!("{path}: expected an array"));
            assert_eq!(actual.len(), expected.len(), "{path}: length");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_matches(actual, expected, &format!("{path}[{index}]"));
            }
        }
        Value::Object(expected) => {
            let actual = actual
                .as_object()
                .unwrap_or_else(|| panic!("{path}: expected an object"));
            let mut keys = actual
                .keys()
                .filter(|key| {
                    !(path.ends_with(".error")
                        && *key == "message"
                        && !expected.contains_key("message"))
                })
                .collect::<Vec<_>>();
            keys.sort();
            let mut wanted = expected.keys().collect::<Vec<_>>();
            wanted.sort();
            assert_eq!(keys, wanted, "{path}: members");
            for (key, expected) in expected {
                assert_matches(&actual[key], expected, &format!("{path}.{key}"));
            }
        }
        expected => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn shared_mixed_dialect_corpus() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/toon-rpc/multi.json");
    let corpus: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(corpus["schemaVersion"], json!("toon-rpc-multi-v1"));
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), CASE_COUNT);

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let raw = case["raw"].as_str().unwrap();
        let (protocol, body) = multi()
            .handle_with_protocol(raw.as_bytes(), case["contentType"].as_str())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let expected_protocol = match case["expect"]["protocol"].as_str().unwrap() {
            "jsonrpc" => Protocol::JsonRpc,
            _ => Protocol::ToonRpc,
        };
        assert_eq!(protocol, expected_protocol, "{name}: protocol");

        let expected = &case["expect"]["response"];
        if expected.is_null() {
            assert!(body.is_empty(), "{name}: no response");
            continue;
        }
        let text = String::from_utf8(body).unwrap();
        let response = match protocol {
            Protocol::JsonRpc => serde_json::from_str(&text).unwrap(),
            Protocol::ToonRpc => reddb_io_toon::decode(&text).unwrap().to_json_value(),
        };
        assert_matches(&response, expected, name);
    }
}
