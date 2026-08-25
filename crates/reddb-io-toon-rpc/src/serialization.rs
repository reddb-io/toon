use crate::error::{ErrorCode, RpcError};
use crate::protocol::{Call, Message, Notification, Request, Response, TOONRPC_VERSION};
use crate::types::{Id, Params};
use reddb_io_toon::{Array, Value as ToonValue};
use serde_json::Value;

const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

pub fn from_wire(raw: &[u8]) -> Result<Message, RpcError> {
    let toon_value = decode_wire_value(raw)?;
    if let ToonValue::Array(Array::List(entries)) = &toon_value {
        return Ok(message_from_toon_batch(entries));
    }
    if let Err(reason) = validate_toon_value(&toon_value) {
        return Ok(Message::Invalid(reason));
    }

    let json_value = toon_value.to_json_value();
    Ok(message_from_value(json_value))
}

/// Decode one response without classifying request-shaped unknown members first.
pub fn response_from_wire(raw: &[u8]) -> Result<Response, RpcError> {
    response_from_value(&decode_wire_value(raw)?)
}

/// Validate one already-decoded document, or one batch entry, as a response
/// envelope. Batch entries are validated independently so a valid sibling is
/// never lost to an invalid neighbour.
pub fn response_from_value(toon_value: &ToonValue) -> Result<Response, RpcError> {
    validate_toon_value(toon_value).map_err(RpcError::InvalidRequest)?;
    let value = toon_value.to_json_value();
    if !value.is_object() {
        return Err(RpcError::InvalidRequest(
            "response must be an object".into(),
        ));
    }
    serde_json::from_value(value).map_err(|error| RpcError::InvalidRequest(error.to_string()))
}

pub(crate) fn decode_wire_value(raw: &[u8]) -> Result<ToonValue, RpcError> {
    let text = std::str::from_utf8(raw)
        .map_err(|e| RpcError::ParseError(format!("invalid UTF-8: {}", e)))?;
    reddb_io_toon::decode(text)
        .map_err(|e| RpcError::ParseError(format!("TOON parse error: {}", e.message())))
}

fn message_from_toon_batch(entries: &[ToonValue]) -> Message {
    if entries.is_empty() {
        return Message::Invalid("empty batch".into());
    }

    let validated = entries
        .iter()
        .map(|entry| validate_toon_value(entry).map(|()| entry.to_json_value()))
        .collect::<Vec<_>>();
    if validated.iter().any(Result::is_err) {
        return Message::Batch(
            validated
                .into_iter()
                .map(|entry| match entry {
                    Ok(Value::Object(object)) => decode_call(object),
                    Ok(_) => Call::Invalid("batch entry must be an object".into()),
                    Err(reason) => Call::Invalid(reason),
                })
                .collect(),
        );
    }

    decode_batch(
        validated
            .into_iter()
            .map(Result::unwrap)
            .collect::<Vec<_>>(),
    )
}

pub fn to_wire(msg: &Message) -> Result<Vec<u8>, RpcError> {
    validate_outbound_message(msg).map_err(RpcError::SerializationError)?;
    let json_value = serde_json::to_value(msg)
        .map_err(|e| RpcError::SerializationError(format!("JSON serialize error: {}", e)))?;
    validate_core_value(&json_value).map_err(RpcError::SerializationError)?;

    let toon_value = ToonValue::from_json_value(json_value);

    let text = reddb_io_toon::encode(&toon_value)
        .map_err(|e| RpcError::SerializationError(format!("TOON encode error: {}", e)))?;

    Ok(text.into_bytes())
}

fn message_from_value(value: Value) -> Message {
    match value {
        Value::Object(object) => decode_single(object),
        Value::Array(entries) if entries.is_empty() => Message::Invalid("empty batch".into()),
        Value::Array(entries) => decode_batch(entries),
        _ => Message::Invalid("request must be an object or non-empty array".into()),
    }
}

fn decode_single(object: serde_json::Map<String, Value>) -> Message {
    if object.contains_key("method") {
        return Message::Single(decode_call(object));
    }
    if object.contains_key("result") || object.contains_key("error") {
        return match serde_json::from_value(Value::Object(object)) {
            Ok(response) => Message::SingleResponse(response),
            Err(error) => Message::Invalid(error.to_string()),
        };
    }
    Message::Invalid("request is missing method".into())
}

fn decode_batch(entries: Vec<Value>) -> Message {
    let is_response = entries.iter().any(|entry| {
        entry
            .as_object()
            .is_some_and(|object| object.contains_key("result") || object.contains_key("error"))
    }) && !entries.iter().any(|entry| {
        entry
            .as_object()
            .is_some_and(|object| object.contains_key("method"))
    });

    if is_response {
        let responses = entries
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<Result<Vec<Response>, _>>();
        if let Ok(responses) = responses {
            return Message::BatchResponse(responses);
        }
    }

    Message::Batch(
        entries
            .into_iter()
            .map(|entry| match entry {
                Value::Object(object) => decode_call(object),
                _ => Call::Invalid("batch entry must be an object".into()),
            })
            .collect(),
    )
}

fn validate_outbound_message(message: &Message) -> Result<(), String> {
    match message {
        Message::Single(call) => validate_outbound_call(call),
        Message::Batch(calls) if calls.is_empty() => Err("request batch must not be empty".into()),
        Message::Batch(calls) => calls.iter().try_for_each(validate_outbound_call),
        Message::SingleResponse(response) => validate_outbound_response(response),
        Message::BatchResponse(responses) if responses.is_empty() => {
            Err("response batch must not be empty".into())
        }
        Message::BatchResponse(responses) => {
            responses.iter().try_for_each(validate_outbound_response)
        }
        Message::Invalid(_) => Err("cannot encode an invalid RPC document".into()),
    }
}

fn validate_outbound_call(call: &Call) -> Result<(), String> {
    match call {
        Call::Request(request) => validate_version_and_method(&request.toonrpc, &request.method),
        Call::Notification(notification) => {
            validate_version_and_method(&notification.toonrpc, &notification.method)
        }
        Call::Invalid(_) => Err("cannot encode an invalid request".into()),
    }
}

fn validate_version_and_method(version: &str, method: &str) -> Result<(), String> {
    if version != TOONRPC_VERSION {
        return Err("invalid toonrpc version".into());
    }
    if method.is_empty() {
        return Err("method must be a non-empty string".into());
    }
    Ok(())
}

fn validate_outbound_response(response: &Response) -> Result<(), String> {
    if response.toonrpc != TOONRPC_VERSION {
        return Err("invalid toonrpc version".into());
    }
    match (&response.result, &response.error) {
        (Some(_), None) => Ok(()),
        (None, Some(error)) => validate_outbound_error_code(error.code),
        _ => Err("response must contain exactly one of result and error".into()),
    }
}

fn validate_outbound_error_code(code: ErrorCode) -> Result<(), String> {
    match code {
        ErrorCode::ServerError(offset) if !ErrorCode::is_valid_server_offset(offset) => {
            Err("server error offset must be between 0 and 99".into())
        }
        _ => Ok(()),
    }
}

fn decode_call(mut object: serde_json::Map<String, Value>) -> Call {
    let version_is_valid = matches!(
        object.remove("toonrpc"),
        Some(Value::String(version)) if version == TOONRPC_VERSION
    );
    if !version_is_valid {
        return Call::Invalid("invalid toonrpc version".into());
    }

    let method = match object.remove("method") {
        Some(Value::String(method)) if !method.is_empty() => method,
        _ => return Call::Invalid("method must be a non-empty string".into()),
    };
    let params = match object.remove("params") {
        None => Params::Absent,
        Some(Value::Array(values)) => Params::ByPosition(values),
        Some(Value::Object(values)) => Params::ByName(values),
        Some(_) => return Call::Invalid("params must be an array or object".into()),
    };

    match object.remove("id") {
        Some(value) => match decode_id(value) {
            Ok(id) => Call::Request(Request {
                toonrpc: TOONRPC_VERSION.into(),
                method,
                params,
                id,
            }),
            Err(reason) => Call::Invalid(reason),
        },
        None => Call::Notification(Notification {
            toonrpc: TOONRPC_VERSION.into(),
            method,
            params,
        }),
    }
}

fn decode_id(value: Value) -> Result<Id, String> {
    match value {
        Value::Null => Ok(Id::Null),
        Value::String(value) => Ok(Id::String(value)),
        Value::Number(value) => value
            .as_i64()
            .filter(|value| value.unsigned_abs() <= MAX_SAFE_INTEGER as u64)
            .map(Id::Number)
            .ok_or_else(|| "id must be a safe integer, string, or null".into()),
        _ => Err("id must be a safe integer, string, or null".into()),
    }
}

fn validate_toon_value(value: &ToonValue) -> Result<(), String> {
    match value {
        ToonValue::Array(Array::List(values)) => values.iter().try_for_each(validate_toon_value),
        ToonValue::Object(object) => object.values().try_for_each(validate_toon_value),
        ToonValue::Number(raw) => validate_number(raw),
        ToonValue::Bool(_) | ToonValue::Null | ToonValue::String(_) => Ok(()),
    }
}

pub(crate) fn validate_core_value(value: &Value) -> Result<(), String> {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(values) => pending.extend(values),
            Value::Object(object) => pending.extend(object.values()),
            Value::Number(number) => validate_number(&number.to_string())?,
            Value::Bool(_) | Value::Null | Value::String(_) => {}
        }
    }
    Ok(())
}

pub(crate) fn validate_response_depth(response: &Response, in_batch: bool) -> Result<(), String> {
    let response_depth = usize::from(in_batch);
    if response_depth > reddb_io_toon::DEFAULT_MAX_DEPTH {
        return Err("maximum TOON nesting depth exceeded".into());
    }

    if let Some(result) = &response.result {
        validate_value_from_object(result, response_depth)?;
    }
    if let Some(error) = &response.error {
        let error_depth = response_depth + 1;
        if error_depth > reddb_io_toon::DEFAULT_MAX_DEPTH {
            return Err("maximum TOON nesting depth exceeded".into());
        }
        if let Some(data) = &error.data {
            validate_value_from_object(data, error_depth)?;
        }
    }
    Ok(())
}

fn validate_value_from_object(value: &Value, parent_depth: usize) -> Result<(), String> {
    let depth = match value {
        Value::Object(_) => parent_depth + 1,
        Value::Array(_) => parent_depth,
        _ => return Ok(()),
    };
    validate_value_depth(value, depth)
}

fn validate_value_depth(value: &Value, depth: usize) -> Result<(), String> {
    let mut pending = vec![(value, depth)];
    while let Some((value, depth)) = pending.pop() {
        if depth > reddb_io_toon::DEFAULT_MAX_DEPTH {
            return Err("maximum TOON nesting depth exceeded".into());
        }
        match value {
            Value::Object(object) => {
                for value in object.values() {
                    match value {
                        Value::Object(_) => pending.push((value, depth + 1)),
                        Value::Array(_) => pending.push((value, depth)),
                        _ => {}
                    }
                }
            }
            Value::Array(values) => {
                pending.extend(values.iter().filter_map(|value| match value {
                    Value::Object(_) | Value::Array(_) => Some((value, depth + 1)),
                    _ => None,
                }));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_number(raw: &str) -> Result<(), String> {
    let number = raw
        .parse::<f64>()
        .map_err(|_| format!("invalid core number: {raw}"))?;
    if !number.is_finite() {
        return Err(format!("core number must be finite: {raw}"));
    }
    if number.fract() == 0.0 && number.abs() > MAX_SAFE_INTEGER {
        return Err(format!("core integer is outside the safe range: {raw}"));
    }
    Ok(())
}
