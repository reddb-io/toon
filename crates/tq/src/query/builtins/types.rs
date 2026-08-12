use reddb_io_toon::Value;

use super::super::ast::Expr;
use super::super::eval::{self, Env};
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("arrays", 0, call_arrays),
    Builtin::new("booleans", 0, call_booleans),
    Builtin::new("infinite", 0, call_infinite),
    Builtin::new("isnan", 0, call_isnan),
    Builtin::new("isinfinite", 0, call_isinfinite),
    Builtin::new("iterables", 0, call_iterables),
    Builtin::new("length", 0, eval::call_length),
    Builtin::new("nan", 0, call_nan),
    Builtin::new("nulls", 0, call_nulls),
    Builtin::new("numbers", 0, call_numbers),
    Builtin::new("objects", 0, call_objects),
    Builtin::new("scalars", 0, call_scalars),
    Builtin::new("strings", 0, call_strings),
    Builtin::new("toarray", 0, call_toarray).divergent("`toarray/0` is not defined by jq 1.7.1"),
    Builtin::new("type", 0, call_type),
    Builtin::new("values", 0, call_values),
];

fn call_arrays(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(input, matches!(input, Value::Array(_))))
}

fn call_booleans(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(input, matches!(input, Value::Bool(_))))
}

fn call_infinite(_: &[Expr], _: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(vec![Value::Number("inf".to_owned())])
}

fn call_isnan(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let is_nan = match input {
        Value::Number(value) => value.parse::<f64>().is_ok_and(f64::is_nan),
        _ => false,
    };
    Ok(vec![Value::Bool(is_nan)])
}

fn call_isinfinite(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let is_infinite = match input {
        Value::Number(value) => value.parse::<f64>().is_ok_and(f64::is_infinite),
        _ => false,
    };
    Ok(vec![Value::Bool(is_infinite)])
}

fn call_nan(_: &[Expr], _: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(vec![Value::Number("NaN".to_owned())])
}

fn call_iterables(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(
        input,
        matches!(input, Value::Array(_) | Value::Object(_)),
    ))
}

fn call_type(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let name = match input {
        Value::Array(_) => "array",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Number(_) => "number",
        Value::Object(_) => "object",
        Value::String(_) => "string",
    };
    Ok(vec![Value::String(name.to_owned())])
}

fn call_values(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(input, !matches!(input, Value::Null)))
}

fn call_nulls(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(input, matches!(input, Value::Null)))
}

fn call_numbers(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(input, matches!(input, Value::Number(_))))
}

fn call_objects(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(input, matches!(input, Value::Object(_))))
}

fn call_scalars(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(
        input,
        !matches!(input, Value::Array(_) | Value::Object(_)),
    ))
}

fn call_strings(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(select(input, matches!(input, Value::String(_))))
}

fn call_toarray(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let array = match input {
        Value::Array(_) => input.clone(),
        _ => Value::Array(reddb_io_toon::Array::List(vec![input.clone()])),
    };
    Ok(vec![array])
}

fn select(input: &Value, matches: bool) -> Vec<Value> {
    matches.then(|| input.clone()).into_iter().collect()
}
