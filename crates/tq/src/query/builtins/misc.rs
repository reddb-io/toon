//! Builtins that talk to the run itself: its trace output, its exit status,
//! and the documents it has not read yet.

use reddb_io_toon::Value;

use super::super::ast::Expr;
use super::super::eval::{self, Env};
use super::super::halt::Halt;
use super::Builtin;

/// jq's exit status for a `halt_error` that was not given one.
const HALT_ERROR_STATUS: u8 = 5;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("debug", 0, call_debug),
    Builtin::new("debug", 1, call_debug_messages),
    Builtin::new("halt", 0, call_halt)
        .divergent("`halt` cancels the output still pending for the document; jq 1.7.1 has already written it"),
    Builtin::new("halt_error", 0, call_halt_error),
    Builtin::new("halt_error", 1, call_halt_error_status),
    Builtin::new("input", 0, call_input),
    Builtin::new("inputs", 0, call_inputs),
    Builtin::new("stderr", 0, call_stderr),
];

fn call_debug(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    trace(input);
    Ok(vec![input.clone()])
}

/// `debug(msgs)` traces what its argument produces and leaves the input alone,
/// exactly as jq's `(msgs | debug | empty), .` does.
fn call_debug_messages(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    for message in arguments[0].eval(input, env)? {
        trace(&message);
    }
    Ok(vec![input.clone()])
}

fn call_stderr(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    eprint!("{}", raw_text(input));
    Ok(vec![input.clone()])
}

fn call_halt(_: &[Expr], _: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Err(Halt::raise(0, String::new()))
}

fn call_halt_error(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Err(Halt::raise(HALT_ERROR_STATUS, halt_error_text(input)))
}

fn call_halt_error_status(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let Some(status) = arguments[0].eval(input, env)?.into_iter().next() else {
        return Ok(Vec::new());
    };
    // jq raises this one against the input rather than the offending argument,
    // so the diagnostic names the value the filter was applied to.
    let Value::Number(number) = &status else {
        return Err(format!(
            "{} ({}) halt_error/1: number required",
            eval::value_kind(input),
            compact(input)
        ));
    };
    Err(Halt::raise(exit_status(number)?, halt_error_text(input)))
}

fn call_input(_: &[Expr], _: &Value, env: &Env) -> Result<Vec<Value>, String> {
    match env.next_input() {
        Some(value) => Ok(vec![value?]),
        None => Err("No more inputs".to_owned()),
    }
}

/// Everything the reader has left. tq evaluates a filter to completion rather
/// than streaming it, so the remaining rows are drawn when `inputs` runs; what
/// laziness buys here is that they come from the live reader, so rows the loop
/// already handled are not repeated and the stream is never slurped twice.
fn call_inputs(_: &[Expr], _: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let mut values = Vec::new();
    while let Some(value) = env.next_input() {
        values.push(value?);
    }
    Ok(values)
}

fn trace(value: &Value) {
    eprintln!(
        "{}",
        compact(&Value::Array(reddb_io_toon::Array::List(vec![
            Value::String("DEBUG:".to_owned()),
            value.clone(),
        ])))
    );
}

/// jq writes a string payload as-is and anything else as compact JSON. `stderr`
/// stops there; `halt_error` ends a non-string payload with a newline.
fn raw_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        value => compact(value),
    }
}

fn halt_error_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        value => format!("{}\n", compact(value)),
    }
}

fn compact(value: &Value) -> String {
    serde_json::to_string(&value.to_json_value()).expect("tq values always serialize as JSON")
}

/// jq passes the requested status to `exit`, which keeps only its low eight
/// bits, so `halt_error(300)` leaves 44 behind.
fn exit_status(number: &str) -> Result<u8, String> {
    let status = eval::parse_number(number)?;
    Ok(status as i64 as u8)
}
