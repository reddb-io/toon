use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use jsonschema::{Draft, JSONSchema};
use reddb_io_toon::{Array as ToonArray, Value as ToonValue};
use reddb_io_toon_rpc::{response_from_wire, Dispatcher, Error, ErrorCode, Id, Params, RpcError};
use serde::Deserialize;
use serde_json::{Map, Value};

const SCHEMA_VERSION: &str = "toon-rpc-fixtures-v1";
const PROTOCOL_VERSION: &str = "1.0";
const CHECKPOINT_VERSION: &str = "4.1.1";
const CHECKPOINT_REPOSITORY: &str = "toon-format/spec";
const CHECKPOINT_REVISION: &str = "62f16b369408180f1faf1cba7da1b46d1f336f12";
const CASE_COUNT: usize = 66;
const SERVER_COUNT: usize = 43;
const CLIENT_COUNT: usize = 23;
const MAX_SAFE_ID: u64 = 9_007_199_254_740_991;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    #[serde(rename = "$schema")]
    schema: String,
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    #[serde(rename = "toonCheckpoint")]
    toon_checkpoint: Checkpoint,
    handlers: BTreeMap<String, Value>,
    valid: Vec<Case>,
    malformed: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    version: String,
    repository: String,
    revision: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    direction: String,
    encoding: String,
    input: Map<String, Value>,
    expect: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum TypedId {
    Null,
    String(String),
    Number(i64),
}

enum ClientOutcome {
    Accept,
    Reject(&'static str),
    Batch {
        settled: Vec<Value>,
        rejected: Vec<(usize, &'static str)>,
        remaining: Vec<Value>,
    },
}

#[test]
fn shared_toon_rpc_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_dir = root.join("tests/corpus/toon-rpc");
    let schema: Value = read_json(&corpus_dir.join("fixtures.schema.json"));
    let corpus_value: Value = read_json(&corpus_dir.join("contract.json"));
    validate_contract_schema(&schema, &corpus_value)
        .unwrap_or_else(|error| panic!("contract schema validation failed:\n{error}"));
    let corpus: Corpus = serde_json::from_value(corpus_value)
        .unwrap_or_else(|error| panic!("strict contract decode failed: {error}"));

    preflight(&schema, &corpus);

    let mut executed = 0;
    for case in corpus.valid.iter().chain(&corpus.malformed) {
        let raw = materialize(case);
        match case.direction.as_str() {
            "server" => run_server_case(case, &corpus.handlers, &raw),
            "client" => run_client_case(case, &raw),
            other => panic!("{}: unknown direction {other}", case.name),
        }
        executed += 1;
    }
    assert_eq!(executed, CASE_COUNT);
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn validate_contract_schema(schema: &Value, instance: &Value) -> Result<(), String> {
    let compiled = JSONSchema::options()
        .with_draft(Draft::Draft202012)
        .compile(schema)
        .map_err(|error| format!("schema compile error: {error}"))?;
    if let Err(errors) = compiled.validate(instance) {
        let details = errors
            .map(|error| {
                format!(
                    "instance {} against schema {}: {error}",
                    error.instance_path, error.schema_path
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Err(details);
    }
    Ok(())
}

#[test]
fn schema_rejects_contract_mutations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_dir = root.join("tests/corpus/toon-rpc");
    let schema: Value = read_json(&corpus_dir.join("fixtures.schema.json"));
    let corpus: Value = read_json(&corpus_dir.join("contract.json"));

    let mut unknown = corpus.clone();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("unknown".into(), Value::Bool(true));
    assert_schema_rejected(&schema, &unknown, "unknown field");

    let mut version = corpus.clone();
    version["schemaVersion"] = Value::String("wrong".into());
    assert_schema_rejected(&schema, &version, "schemaVersion");

    let mut name = corpus.clone();
    contract_case_mut(&mut name, "request/positional-params")["name"] =
        Value::String("Invalid Name".into());
    assert_schema_rejected(&schema, &name, "name pattern");

    let mut data = corpus.clone();
    contract_case_mut(&mut data, "error/application-code-without-data")["expect"]
        .as_object_mut()
        .unwrap()
        .insert("data".into(), Value::Null);
    assert_schema_rejected(&schema, &data, "data presence");

    let mut ordered = corpus;
    contract_case_mut(&mut ordered, "batch/mixed-request-and-notification")["expect"]["ordered"] =
        Value::Bool(true);
    assert_schema_rejected(&schema, &ordered, "ordered batch");
}

fn assert_schema_rejected(schema: &Value, instance: &Value, mutation: &str) {
    match validate_contract_schema(schema, instance) {
        Ok(()) => panic!("schema accepted {mutation} mutation"),
        Err(error) => assert!(!error.is_empty(), "{mutation}: missing schema diagnostic"),
    }
}

fn contract_case_mut<'a>(corpus: &'a mut Value, name: &str) -> &'a mut Value {
    let section = ["valid", "malformed"]
        .into_iter()
        .find(|section| {
            corpus[*section]
                .as_array()
                .unwrap()
                .iter()
                .any(|case| case["name"] == name)
        })
        .unwrap_or_else(|| panic!("missing contract case {name}"));
    corpus[section]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|case| case["name"] == name)
        .unwrap()
}

fn preflight(schema: &Value, corpus: &Corpus) {
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(
        schema["$id"],
        "https://reddb.io/schemas/toon-rpc-fixtures-v1.json"
    );
    assert_eq!(schema["$defs"]["input"]["type"], "object");
    assert_eq!(schema["$defs"]["case"]["type"], "object");
    assert_eq!(
        schema["properties"]["schemaVersion"]["const"],
        SCHEMA_VERSION
    );
    assert_eq!(
        schema["properties"]["protocolVersion"]["const"],
        PROTOCOL_VERSION
    );
    assert_eq!(
        schema["$defs"]["toonCheckpoint"]["properties"]["version"]["const"],
        CHECKPOINT_VERSION
    );
    assert_eq!(
        schema["$defs"]["toonCheckpoint"]["properties"]["repository"]["const"],
        CHECKPOINT_REPOSITORY
    );
    assert_eq!(
        schema["$defs"]["toonCheckpoint"]["properties"]["revision"]["const"],
        CHECKPOINT_REVISION
    );

    assert_eq!(corpus.schema, "./fixtures.schema.json");
    assert_eq!(corpus.schema_version, SCHEMA_VERSION);
    assert_eq!(corpus.protocol_version, PROTOCOL_VERSION);
    assert_eq!(corpus.toon_checkpoint.version, CHECKPOINT_VERSION);
    assert_eq!(corpus.toon_checkpoint.repository, CHECKPOINT_REPOSITORY);
    assert_eq!(corpus.toon_checkpoint.revision, CHECKPOINT_REVISION);
    validate_handlers(&corpus.handlers);

    let cases = corpus
        .valid
        .iter()
        .chain(&corpus.malformed)
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), CASE_COUNT);
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.direction == "server")
            .count(),
        SERVER_COUNT
    );
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.direction == "client")
            .count(),
        CLIENT_COUNT
    );

    let mut names = HashSet::new();
    for case in cases {
        assert!(
            names.insert(&case.name),
            "duplicate case name: {}",
            case.name
        );
        assert_eq!(case.encoding, "toon", "{}: encoding", case.name);
        assert!(
            matches!(case.direction.as_str(), "server" | "client"),
            "{}: direction",
            case.name
        );

        let mut source_keys = case
            .input
            .keys()
            .filter(|key| key.as_str() != "pendingIds")
            .map(String::as_str)
            .collect::<Vec<_>>();
        source_keys.sort_unstable();
        let source_form = source_keys.join("+");
        assert!(
            matches!(
                source_form.as_str(),
                "bytesBase64" | "value" | "value+wire" | "wire"
            ),
            "{}: source form must be wire, value, bytesBase64, or wire+value",
            case.name
        );

        if let Some(wire) = case.input.get("wire") {
            assert!(wire.is_string(), "{}: wire must be a string", case.name);
        }
        if let Some(value) = case.input.get("value") {
            validate_core_fixture_value(value, &case.name);
        }
        if let Some(encoded) = case.input.get("bytesBase64") {
            assert!(
                encoded.is_string(),
                "{}: bytesBase64 must be a string",
                case.name
            );
        }

        if let (Some(Value::String(wire)), Some(value)) =
            (case.input.get("wire"), case.input.get("value"))
        {
            assert_eq!(
                decode_toon_value(wire.as_bytes())
                    .unwrap_or_else(|error| panic!("{}: paired wire: {error}", case.name)),
                *value,
                "{}: paired wire does not decode to value",
                case.name
            );
        }

        if let Some(Value::String(encoded)) = case.input.get("bytesBase64") {
            assert!(encoded.len() >= 4, "{}: base64 is too short", case.name);
            let decoded = BASE64
                .decode(encoded)
                .unwrap_or_else(|error| panic!("{}: base64: {error}", case.name));
            assert_eq!(
                BASE64.encode(decoded),
                *encoded,
                "{}: non-canonical base64",
                case.name
            );
        }

        if case.direction == "server" {
            required_u64(&case.expect, "callCount", &case.name);
        } else {
            assert!(
                !case.expect.contains_key("callCount") && !case.expect.contains_key("calls"),
                "{}: client expectation has call counts",
                case.name
            );
        }
        if let Some(calls) = case.expect.get("calls") {
            let calls = calls
                .as_object()
                .unwrap_or_else(|| panic!("{}: calls must be an object", case.name));
            let sum: u64 = calls
                .iter()
                .map(|(method, count)| {
                    assert!(
                        corpus.handlers.contains_key(method),
                        "{}: calls contains undeclared handler {method}",
                        case.name
                    );
                    count
                        .as_u64()
                        .filter(|count| *count > 0)
                        .unwrap_or_else(|| panic!("{}: invalid calls count", case.name))
                })
                .sum();
            assert_eq!(
                sum,
                required_u64(&case.expect, "callCount", &case.name),
                "{}: calls sum",
                case.name
            );
        }

        match case.direction.as_str() {
            "client" => {
                let pending = required_array(&case.input, "pendingIds", &case.name);
                let mut unique = HashSet::new();
                for id in pending {
                    assert!(
                        unique.insert(typed_id(id, &case.name)),
                        "{}: duplicate pending id",
                        case.name
                    );
                }
            }
            "server" => assert!(
                !case.input.contains_key("pendingIds"),
                "{}: server case has pendingIds",
                case.name
            ),
            _ => unreachable!(),
        }
    }
}

fn validate_handlers(handlers: &BTreeMap<String, Value>) {
    assert!(!handlers.is_empty(), "handlers must not be empty");
    for (method, definition) in handlers {
        assert!(!method.is_empty(), "handler method must not be empty");
        let object = definition
            .as_object()
            .unwrap_or_else(|| panic!("handler {method}: definition must be an object"));
        let kind = required_str(object, "kind", method);
        let required = match kind {
            "result" => {
                let value = object
                    .get("value")
                    .unwrap_or_else(|| panic!("handler {method}: missing value"));
                validate_core_fixture_value(value, method);
                &["kind", "value"][..]
            }
            "error" => {
                let code = required_i64(object, "code", method);
                i32::try_from(code)
                    .unwrap_or_else(|_| panic!("handler {method}: code must be signed 32-bit"));
                required_str(object, "message", method);
                if let Some(data) = object.get("data") {
                    validate_core_fixture_value(data, method);
                }
                if object.contains_key("data") {
                    &["code", "data", "kind", "message"][..]
                } else {
                    &["code", "kind", "message"][..]
                }
            }
            "echo-params" | "internal-error" => &["kind"][..],
            "reject-params" => {
                required_str(object, "message", method);
                &["kind", "message"][..]
            }
            other => panic!("handler {method}: unknown kind {other}"),
        };
        let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
        let required = required.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(actual, required, "handler {method}: exact members");
    }
}

fn validate_core_fixture_value(value: &Value, context: &str) {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(values) => pending.extend(values),
            Value::Object(object) => pending.extend(object.values()),
            Value::Number(number) if number.is_i64() => assert!(
                number.as_i64().unwrap().unsigned_abs() <= MAX_SAFE_ID,
                "{context}: unsafe integer fixture value"
            ),
            Value::Number(number) if number.is_u64() => assert!(
                number.as_u64().unwrap() <= MAX_SAFE_ID,
                "{context}: unsafe integer fixture value"
            ),
            Value::Number(number) => assert!(
                number.as_f64().is_some_and(f64::is_finite),
                "{context}: non-finite fixture value"
            ),
            Value::Null | Value::Bool(_) | Value::String(_) => {}
        }
    }
}

fn materialize(case: &Case) -> Vec<u8> {
    if let Some(Value::String(wire)) = case.input.get("wire") {
        return wire.as_bytes().to_vec();
    }
    if let Some(Value::String(encoded)) = case.input.get("bytesBase64") {
        return BASE64
            .decode(encoded)
            .unwrap_or_else(|error| panic!("{}: base64: {error}", case.name));
    }
    let value = case
        .input
        .get("value")
        .unwrap_or_else(|| panic!("{}: missing source", case.name));
    reddb_io_toon::encode(&ToonValue::from_json_value(value.clone()))
        .unwrap_or_else(|error| panic!("{}: value encode: {error:?}", case.name))
        .into_bytes()
}

fn run_server_case(case: &Case, handlers: &BTreeMap<String, Value>, raw: &[u8]) {
    let calls = Arc::new(Mutex::new(BTreeMap::<String, u64>::new()));
    let mut dispatcher = Dispatcher::new();

    for (method, definition) in handlers {
        let method_name = method.clone();
        let definition = definition.clone();
        let calls = Arc::clone(&calls);
        dispatcher.register(method.clone(), move |params, _id| {
            *calls
                .lock()
                .expect("call counter mutex poisoned")
                .entry(method_name.clone())
                .or_default() += 1;
            invoke_handler(&definition, params)
        });
    }

    let response = dispatcher
        .dispatch(raw)
        .unwrap_or_else(|error| panic!("{}: dispatch failed: {error}", case.name));
    let actual_calls = calls.lock().expect("call counter mutex poisoned").clone();
    check_calls(case, &actual_calls);

    match required_str(&case.expect, "kind", &case.name) {
        "no-response" => assert!(response.is_empty(), "{}: expected no response", case.name),
        "success" | "error" => {
            assert!(!response.is_empty(), "{}: missing response", case.name);
            let value = decode_toon_value(&response)
                .unwrap_or_else(|error| panic!("{}: response decode: {error}", case.name));
            assert!(
                response_matches(&value, &case.expect, true),
                "{}: response mismatch\nactual: {value:#}\nexpected: {:#}",
                case.name,
                Value::Object(case.expect.clone())
            );
        }
        "batch" => {
            assert_eq!(
                case.expect.get("ordered").and_then(Value::as_bool),
                Some(false),
                "{}: batch comparison requires ordered=false",
                case.name
            );
            let value = decode_toon_value(&response)
                .unwrap_or_else(|error| panic!("{}: response decode: {error}", case.name));
            let actual = value
                .as_array()
                .unwrap_or_else(|| panic!("{}: expected batch response", case.name));
            let expected = required_array(&case.expect, "responses", &case.name);
            assert_unordered_responses(actual, expected, &case.name);
        }
        kind => panic!("{}: invalid server expectation {kind}", case.name),
    }
}

fn invoke_handler(definition: &Value, params: Params) -> Result<Value, RpcError> {
    let object = definition
        .as_object()
        .expect("handler definition must be object");
    match required_str(object, "kind", "handler") {
        "result" => Ok(object.get("value").expect("result handler value").clone()),
        "echo-params" => Ok(match params {
            Params::ByPosition(values) => Value::Array(values),
            Params::ByName(values) => Value::Object(values),
            Params::Absent => Value::Null,
        }),
        "reject-params" => Err(RpcError::InvalidParams(
            required_str(object, "message", "handler").to_owned(),
        )),
        "internal-error" => Err(RpcError::InternalError("fixture internal error".into())),
        "error" => {
            let code = required_i64(object, "code", "handler");
            let code = i32::try_from(code).expect("handler code must be i32");
            let error = Error {
                code: ErrorCode::from_code(code).expect("handler code must be i32"),
                message: required_str(object, "message", "handler").to_owned(),
                data: object.get("data").cloned(),
            };
            Err(RpcError::ResponseError(error))
        }
        kind => panic!("unknown fixture handler kind {kind}"),
    }
}

fn check_calls(case: &Case, actual: &BTreeMap<String, u64>) {
    let actual_total: u64 = actual.values().sum();
    assert_eq!(
        actual_total,
        required_u64(&case.expect, "callCount", &case.name),
        "{}: callCount",
        case.name
    );
    if let Some(expected) = case.expect.get("calls") {
        let expected = expected.as_object().expect("calls must be object");
        let expected = expected
            .iter()
            .map(|(method, count)| (method.clone(), count.as_u64().expect("calls count")))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(*actual, expected, "{}: exact calls map", case.name);
    }
}

fn run_client_case(case: &Case, raw: &[u8]) {
    let pending_values = required_array(&case.input, "pendingIds", &case.name).to_vec();
    let pending = pending_values
        .iter()
        .map(|id| (typed_id(id, &case.name), id.clone()))
        .collect::<Vec<_>>();
    let outcome = client_oracle(raw, pending);
    let kind = required_str(&case.expect, "kind", &case.name);

    match (kind, outcome) {
        ("accept", ClientOutcome::Accept) => {}
        ("reject", ClientOutcome::Reject(actual)) => assert_eq!(
            actual,
            required_str(&case.expect, "reason", &case.name),
            "{}: rejection reason",
            case.name
        ),
        (
            "client-batch",
            ClientOutcome::Batch {
                settled,
                rejected,
                remaining,
            },
        ) => {
            let expected_settled = required_array(&case.expect, "settled", &case.name);
            assert_eq!(
                settled.len(),
                expected_settled.len(),
                "{}: settled count",
                case.name
            );
            for (index, (actual, expected)) in settled.iter().zip(expected_settled).enumerate() {
                let expected = expected.as_object().expect("settled matcher object");
                assert!(
                    response_matches(actual, expected, false),
                    "{}: settled response at index {index}",
                    case.name
                );
            }

            let expected_rejected = required_array(&case.expect, "rejected", &case.name)
                .iter()
                .map(|entry| {
                    let entry = entry.as_object().expect("rejected entry object");
                    (
                        required_u64(entry, "index", &case.name) as usize,
                        required_str(entry, "reason", &case.name),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                rejected, expected_rejected,
                "{}: rejected entries",
                case.name
            );
            assert_eq!(
                remaining,
                required_array(&case.expect, "remainingPendingIds", &case.name),
                "{}: remaining pending ids",
                case.name
            );
        }
        (_, ClientOutcome::Reject(reason)) => {
            panic!("{}: unexpected client rejection {reason}", case.name)
        }
        (_, ClientOutcome::Accept) => panic!("{}: unexpected client acceptance", case.name),
        (_, ClientOutcome::Batch { .. }) => panic!("{}: unexpected client batch", case.name),
    }
}

// Harness-only client oracle. It deliberately tests the recovery contract
// without blessing either production client, which does not yet expose the
// required per-entry batch diagnostics.
fn client_oracle(raw: &[u8], pending: Vec<(TypedId, Value)>) -> ClientOutcome {
    let text = match std::str::from_utf8(raw) {
        Ok(text) => text,
        Err(_) => return ClientOutcome::Reject("parse-error"),
    };
    let root = match reddb_io_toon::decode(text) {
        Ok(root) => root,
        Err(_) => return ClientOutcome::Reject("parse-error"),
    };

    match root {
        ToonValue::Array(ToonArray::List(entries)) => {
            if entries.is_empty() {
                return ClientOutcome::Reject("invalid-response");
            }
            client_batch_oracle(entries, pending)
        }
        _ => match response_from_wire(raw).ok() {
            Some(response) => {
                let id = response_id(&response);
                if pending.iter().any(|(pending_id, _)| *pending_id == id) {
                    ClientOutcome::Accept
                } else {
                    ClientOutcome::Reject("unknown-id")
                }
            }
            None => ClientOutcome::Reject("invalid-response"),
        },
    }
}

fn client_batch_oracle(entries: Vec<ToonValue>, pending: Vec<(TypedId, Value)>) -> ClientOutcome {
    let mut pending_map = pending.iter().cloned().collect::<HashMap<TypedId, Value>>();
    let mut settled_ids = HashSet::new();
    let mut settled = Vec::new();
    let mut rejected = Vec::new();

    for (index, entry) in entries.into_iter().enumerate() {
        // Mixed response batches cannot be represented atomically by Message::BatchResponse.
        // Re-encoding each root entry preserves independent validation and recovery.
        let wire = match reddb_io_toon::encode(&entry) {
            Ok(wire) => wire,
            Err(_) => {
                rejected.push((index, "invalid-response"));
                continue;
            }
        };
        let Some(response) = response_from_wire(wire.as_bytes()).ok() else {
            rejected.push((index, "invalid-response"));
            continue;
        };
        let id = response_id(&response);
        if settled_ids.contains(&id) {
            rejected.push((index, "duplicate-id"));
        } else if pending_map.remove(&id).is_some() {
            settled_ids.insert(id);
            settled.push(serde_json::to_value(response).expect("serialize parsed response"));
        } else {
            rejected.push((index, "unknown-id"));
        }
    }

    let remaining = pending
        .into_iter()
        .filter_map(|(id, value)| pending_map.contains_key(&id).then_some(value))
        .collect();
    ClientOutcome::Batch {
        settled,
        rejected,
        remaining,
    }
}

fn response_id(response: &reddb_io_toon_rpc::Response) -> TypedId {
    match &response.id {
        Id::Null => TypedId::Null,
        Id::String(value) => TypedId::String(value.clone()),
        Id::Number(value) => TypedId::Number(*value),
    }
}

fn response_matches(actual: &Value, expected: &Map<String, Value>, generated: bool) -> bool {
    let Some(actual) = actual.as_object() else {
        return false;
    };
    let Some(expected_members) = expected.get("exactMembers").and_then(Value::as_array) else {
        return false;
    };
    let actual_members = actual.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected_members = expected_members
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if actual_members != expected_members
        || actual.get("toonrpc") != Some(&Value::String(PROTOCOL_VERSION.into()))
        || actual.get("id") != expected.get("id")
    {
        return false;
    }

    match expected.get("kind").and_then(Value::as_str) {
        Some("success") => actual.get("result") == expected.get("result"),
        Some("error") => {
            let Some(error) = actual.get("error").and_then(Value::as_object) else {
                return false;
            };
            let valid_code = error
                .get("code")
                .and_then(Value::as_i64)
                .is_some_and(|code| i32::try_from(code).is_ok());
            if !valid_code
                || !error.get("message").is_some_and(Value::is_string)
                || error.get("code") != expected.get("code")
            {
                return false;
            }
            let has_data = error.contains_key("data");
            if Some(has_data) != expected.get("hasData").and_then(Value::as_bool) {
                return false;
            }
            if has_data && error.get("data") != expected.get("data") {
                return false;
            }
            if generated {
                let actual_members = error.keys().map(String::as_str).collect::<BTreeSet<_>>();
                let expected_members = if has_data {
                    BTreeSet::from(["code", "data", "message"])
                } else {
                    BTreeSet::from(["code", "message"])
                };
                if actual_members != expected_members {
                    return false;
                }
            }
            expected
                .get("message")
                .map_or(true, |message| error.get("message") == Some(message))
        }
        _ => false,
    }
}

fn assert_unordered_responses(actual: &[Value], expected: &[Value], case_name: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{case_name}: batch response count"
    );
    let mut unused = actual.iter().collect::<Vec<_>>();
    for matcher in expected {
        let matcher = matcher.as_object().expect("response matcher object");
        let index = unused
            .iter()
            .position(|response| response_matches(response, matcher, true))
            .unwrap_or_else(|| panic!("{case_name}: no response matched {matcher:#?}"));
        unused.remove(index);
    }
    assert!(unused.is_empty(), "{case_name}: unmatched batch responses");
}

fn decode_toon_value(raw: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(raw).map_err(|error| error.to_string())?;
    reddb_io_toon::decode(text)
        .map(|value| value.to_json_value())
        .map_err(|error| error.message().to_owned())
}

fn typed_id(value: &Value, context: &str) -> TypedId {
    match value {
        Value::Null => TypedId::Null,
        Value::String(value) => TypedId::String(value.clone()),
        Value::Number(value) => value
            .as_i64()
            .filter(|value| value.unsigned_abs() <= MAX_SAFE_ID)
            .map(TypedId::Number)
            .unwrap_or_else(|| panic!("{context}: invalid fixture id {value}")),
        _ => panic!("{context}: invalid fixture id {value}"),
    }
}

fn required_array<'a>(object: &'a Map<String, Value>, key: &str, context: &str) -> &'a [Value] {
    object
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{context}: missing array {key}"))
}

fn required_str<'a>(object: &'a Map<String, Value>, key: &str, context: &str) -> &'a str {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{context}: missing string {key}"))
}

fn required_u64(object: &Map<String, Value>, key: &str, context: &str) -> u64 {
    object
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("{context}: missing unsigned integer {key}"))
}

fn required_i64(object: &Map<String, Value>, key: &str, context: &str) -> i64 {
    object
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("{context}: missing integer {key}"))
}
