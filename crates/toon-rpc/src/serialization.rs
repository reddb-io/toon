use crate::error::RpcError;
use crate::protocol::Message;
use reddb_io_toon::Value;

pub fn from_wire(raw: &[u8]) -> Result<Message, RpcError> {
    let text = std::str::from_utf8(raw)
        .map_err(|e| RpcError::ParseError(format!("invalid UTF-8: {}", e)))?;

    let toon_value = reddb_io_toon::decode(text)
        .map_err(|e| RpcError::ParseError(format!("TOON parse error: {}", e.message())))?;

    let json_value = toon_value.to_json_value();

    serde_json::from_value(json_value)
        .map_err(|e| RpcError::ParseError(format!("JSON parse error: {}", e)))
}

pub fn to_wire(msg: &Message) -> Result<Vec<u8>, RpcError> {
    let json_value = serde_json::to_value(msg)
        .map_err(|e| RpcError::SerializationError(format!("JSON serialize error: {}", e)))?;

    let toon_value = Value::from_json_value(json_value);

    let text = reddb_io_toon::encode(&toon_value)
        .map_err(|e| RpcError::SerializationError(format!("TOON encode error: {}", e)))?;

    Ok(text.into_bytes())
}
