use reddb_io_toon::Value;

use super::super::ast::Expr;
use super::super::eval::{self, Env};
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("@base64", 0, call_base64),
    Builtin::new("@base64d", 0, call_base64d),
    Builtin::new("@csv", 0, call_csv),
    Builtin::new("@html", 0, call_html),
    Builtin::new("@json", 0, call_json),
    Builtin::new("@sh", 0, call_sh),
    Builtin::new("@text", 0, call_text),
    Builtin::new("@tsv", 0, call_tsv),
    Builtin::new("@uri", 0, call_uri),
    Builtin::new("fromjson", 0, call_fromjson),
    Builtin::new("tojson", 0, call_json),
    Builtin::new("tonumber", 0, call_tonumber),
    Builtin::new("tostring", 0, call_text),
];

/// The base64 alphabet, in the order its six-bit codes index it.
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The two row formats differ in how they quote a cell and what joins the row,
/// so one implementation covers both.
enum Row {
    Csv,
    Tsv,
}

fn call_text(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(vec![Value::String(text(input))])
}

fn call_json(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(vec![Value::String(json(input))])
}

fn call_csv(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    format_row(input, &Row::Csv)
}

fn call_tsv(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    format_row(input, &Row::Tsv)
}

fn call_html(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    // `&` is replaced first so the entities the other replacements introduce
    // are not escaped a second time.
    let escaped = text(input)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
        .replace('"', "&quot;");
    Ok(vec![Value::String(escaped)])
}

fn call_uri(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let mut escaped = String::new();
    for byte in text(input).bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            escaped.push(char::from(byte));
        } else {
            escaped.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(vec![Value::String(escaped)])
}

fn call_sh(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let words = match input {
        Value::Array(array) => array
            .values()
            .iter()
            .map(shell_word)
            .collect::<Result<Vec<_>, _>>()?,
        value => vec![shell_word(value)?],
    };
    Ok(vec![Value::String(words.join(" "))])
}

fn call_base64(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let bytes = text(input).into_bytes();
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let mut group = [0u8; 3];
        group[..chunk.len()].copy_from_slice(chunk);
        let code = (u32::from(group[0]) << 16) | (u32::from(group[1]) << 8) | u32::from(group[2]);
        for position in 0..4 {
            if position <= chunk.len() {
                encoded.push(char::from(
                    BASE64[(code >> (18 - 6 * position) & 0x3f) as usize],
                ));
            } else {
                encoded.push('=');
            }
        }
    }
    Ok(vec![Value::String(encoded)])
}

/// jq decodes up to the first `=`, rejects any character outside the alphabet,
/// and rejects a final group holding a single character, which carries only six
/// bits and so cannot complete a byte. Bytes that are not valid UTF-8 become
/// replacement characters rather than an error.
fn call_base64d(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let encoded = text(input);
    let mut decoded = Vec::new();
    let mut code = 0u32;
    let mut read = 0usize;

    for byte in encoded.bytes().take_while(|byte| *byte != b'=') {
        let Some(position) = BASE64.iter().position(|entry| *entry == byte) else {
            return Err(string_error(&encoded, "is not valid base64 data"));
        };
        code = (code << 6) | position as u32;
        read += 1;
        if read == 4 {
            decoded.extend_from_slice(&[(code >> 16) as u8, (code >> 8) as u8, code as u8]);
            code = 0;
            read = 0;
        }
    }
    match read {
        1 => return Err(string_error(&encoded, "trailing base64 byte found")),
        2 => decoded.push((code >> 4) as u8),
        3 => decoded.extend_from_slice(&[(code >> 10) as u8, (code >> 2) as u8]),
        _ => {}
    }
    Ok(vec![Value::String(
        String::from_utf8_lossy(&decoded).into_owned(),
    )])
}

/// jq parses the string as JSON and keeps the result only when it is a number,
/// so `"[1]"` is a type error while `"abc"` is a parse error.
fn call_tonumber(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    match input {
        Value::Number(_) => Ok(vec![input.clone()]),
        Value::String(value) => match parse_json(value)? {
            number @ Value::Number(_) => Ok(vec![number]),
            _ => Err(type_error(input, "cannot be parsed as a number")),
        },
        _ => Err(type_error(input, "cannot be parsed as a number")),
    }
}

fn call_fromjson(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let Value::String(value) = input else {
        return Err(type_error(input, "only strings can be parsed"));
    };
    Ok(vec![parse_json(value)?])
}

fn format_row(input: &Value, row: &Row) -> Result<Vec<Value>, String> {
    let Value::Array(array) = input else {
        let label = match row {
            Row::Csv => "cannot be csv-formatted, only array",
            Row::Tsv => "cannot be tsv-formatted, only array",
        };
        return Err(type_error(input, label));
    };

    let mut cells = Vec::new();
    for value in array.values() {
        cells.push(match (row, &value) {
            (_, Value::Null) => String::new(),
            (_, Value::Bool(_) | Value::Number(_)) => json(&value),
            (Row::Csv, Value::String(cell)) => format!("\"{}\"", cell.replace('"', "\"\"")),
            (Row::Tsv, Value::String(cell)) => escape_tsv(cell),
            // jq reports an unformattable tsv cell with its csv wording. The
            // message is part of the parity contract, so tq repeats it.
            _ => return Err(type_error(&value, "is not valid in a csv row")),
        });
    }

    let separator = match row {
        Row::Csv => ",",
        Row::Tsv => "\t",
    };
    Ok(vec![Value::String(cells.join(separator))])
}

/// A tab, newline, carriage return, or backslash cannot appear literally in a
/// tsv cell, so each is written as its two-character escape.
fn escape_tsv(cell: &str) -> String {
    cell.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// One shell word: a string is single-quoted, a scalar stands as it prints, and
/// a nested array or object has no shell spelling at all.
fn shell_word(value: &Value) -> Result<String, String> {
    match value {
        Value::String(word) => Ok(format!("'{}'", word.replace('\'', "'\\''"))),
        Value::Array(_) | Value::Object(_) => {
            Err(type_error(value, "can not be escaped for shell"))
        }
        value => Ok(json(value)),
    }
}

fn parse_json(value: &str) -> Result<Value, String> {
    serde_json::from_str(value)
        .map(Value::from_json_value)
        .map_err(|_| format!("Invalid numeric literal (while parsing '{value}')"))
}

/// `tostring` and `@text`: a string is already text, anything else is its JSON.
fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        value => json(value),
    }
}

fn json(value: &Value) -> String {
    serde_json::to_string(&value.to_json_value()).expect("tq values always serialize as JSON")
}

/// jq names the value it could not format: `kind (json) message`.
fn type_error(value: &Value, message: &str) -> String {
    format!("{} ({}) {message}", eval::value_kind(value), json(value))
}

fn string_error(value: &str, message: &str) -> String {
    type_error(&Value::String(value.to_owned()), message)
}
