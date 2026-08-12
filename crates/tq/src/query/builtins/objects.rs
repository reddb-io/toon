use reddb_io_toon::{Array, Value};

use super::super::ast::Expr;
use super::super::eval;
use super::super::eval::Env;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("from_entries", 0, eval::call_from_entries),
    Builtin::new("has", 1, eval::call_has),
    Builtin::new("keys", 0, eval::call_keys),
    Builtin::new("keys_unsorted", 0, call_keys_unsorted),
    Builtin::new("to_entries", 0, eval::call_to_entries),
    Builtin::new("walk", 1, call_walk),
    Builtin::new("with_entries", 1, call_with_entries),
];

fn call_keys_unsorted(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    if let Value::Array(array) = input {
        return Ok(vec![Value::Array(Array::List(
            (0..array.len())
                .map(|index| Value::Number(index.to_string()))
                .collect(),
        ))]);
    }
    let Value::Object(document) = input else {
        return Err("value has no keys".to_owned());
    };
    let serde_json::Value::Object(map) = document.to_json_value() else {
        unreachable!("document serializes as object");
    };
    Ok(vec![Value::Array(Array::List(
        map.into_iter().map(|(key, _)| Value::String(key)).collect(),
    ))])
}

fn call_with_entries(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let entries = eval::call_to_entries(&[], input, env)?;
    let Value::Array(entries) = &entries[0] else {
        unreachable!("to_entries returns one array");
    };
    let mut mapped = Vec::new();
    for entry in entries.values() {
        mapped.extend(arguments[0].eval(&entry, env)?);
    }
    eval::call_from_entries(&[], &Value::Array(Array::List(mapped)), env)
}

fn call_walk(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    walk_value(input, &arguments[0], env)
}

fn walk_value(input: &Value, filter: &Expr, env: &Env) -> Result<Vec<Value>, String> {
    let walked = match input {
        Value::Array(array) => {
            let mut values = Vec::new();
            for value in array.values() {
                values.extend(walk_value(&value, filter, env)?);
            }
            Value::Array(Array::List(values))
        }
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            let mut walked = serde_json::Map::new();
            for (key, value) in map {
                if let Some(value) = walk_value(&Value::from_json_value(value), filter, env)?
                    .into_iter()
                    .next()
                {
                    walked.insert(key, value.to_json_value());
                }
            }
            Value::from_json_value(serde_json::Value::Object(walked))
        }
        value => value.clone(),
    };
    filter.eval(&walked, env)
}
