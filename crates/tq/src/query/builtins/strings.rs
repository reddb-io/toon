use reddb_io_toon::{Array, Value};

use super::super::ast::Expr;
use super::super::eval::{self, Env};
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("ascii_downcase", 0, call_ascii_downcase),
    Builtin::new("ascii_upcase", 0, call_ascii_upcase),
    Builtin::new("endswith", 1, call_endswith),
    Builtin::new("explode", 0, call_explode),
    Builtin::new("implode", 0, call_implode),
    Builtin::new("join", 1, eval::call_join),
    Builtin::new("ltrimstr", 1, call_ltrimstr),
    Builtin::new("rtrimstr", 1, call_rtrimstr),
    Builtin::new("split", 1, eval::call_split),
    Builtin::new("startswith", 1, call_startswith),
    Builtin::new("trim", 0, call_trim).divergent("`trim/0` is not defined by jq 1.7.1"),
    Builtin::new("trimstr", 1, call_trimstr)
        .divergent("`trimstr/1` is not defined by jq 1.7.1"),
];

fn call_ascii_downcase(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let Value::String(value) = input else {
        return Err("explode input must be a string".to_owned());
    };
    Ok(vec![Value::String(value.to_ascii_lowercase())])
}

fn call_ascii_upcase(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let Value::String(value) = input else {
        return Err("explode input must be a string".to_owned());
    };
    Ok(vec![Value::String(value.to_ascii_uppercase())])
}

fn call_ltrimstr(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    Ok(arguments[0]
        .eval(input, env)?
        .into_iter()
        .map(|prefix| match (input, prefix) {
            (Value::String(value), Value::String(prefix)) => {
                Value::String(value.strip_prefix(&prefix).unwrap_or(value).to_owned())
            }
            _ => input.clone(),
        })
        .collect())
}

fn call_rtrimstr(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    Ok(arguments[0]
        .eval(input, env)?
        .into_iter()
        .map(|suffix| match (input, suffix) {
            (Value::String(value), Value::String(suffix)) => {
                Value::String(value.strip_suffix(&suffix).unwrap_or(value).to_owned())
            }
            _ => input.clone(),
        })
        .collect())
}

fn call_trimstr(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let (value, affix) = string_input_and_arg(arguments, input, env, "trimstr")?;
    let value = value.strip_prefix(&affix).unwrap_or(&value);
    Ok(vec![Value::String(
        value.strip_suffix(&affix).unwrap_or(value).to_owned(),
    )])
}

fn call_trim(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let Value::String(value) = input else {
        return Err("trim input must be a string".to_owned());
    };
    Ok(vec![Value::String(value.trim().to_owned())])
}

fn call_startswith(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let (value, prefix) = string_input_and_arg(arguments, input, env, "startswith()")?;
    Ok(vec![Value::Bool(value.starts_with(&prefix))])
}

fn call_endswith(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let (value, suffix) = string_input_and_arg(arguments, input, env, "endswith()")?;
    Ok(vec![Value::Bool(value.ends_with(&suffix))])
}

fn call_explode(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let Value::String(value) = input else {
        return Err("explode input must be a string".to_owned());
    };
    Ok(vec![Value::Array(Array::List(
        value
            .chars()
            .map(|character| Value::Number(u32::from(character).to_string()))
            .collect(),
    ))])
}

fn call_implode(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let Value::Array(array) = input else {
        return Err("implode input must be an array".to_owned());
    };
    let mut output = String::new();
    for value in array.values() {
        let Value::Number(number) = value else {
            return Err("unicode codepoint needs to be numeric".to_owned());
        };
        let codepoint = number
            .parse::<f64>()
            .map_err(|_| "unicode codepoint needs to be numeric".to_owned())?;
        let character =
            if codepoint.is_finite() && codepoint >= 0.0 && codepoint <= f64::from(u32::MAX) {
                char::from_u32(codepoint.trunc() as u32).unwrap_or(char::REPLACEMENT_CHARACTER)
            } else {
                char::REPLACEMENT_CHARACTER
            };
        output.push(character);
    }
    Ok(vec![Value::String(output)])
}

fn string_input_and_arg(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
    builtin: &str,
) -> Result<(String, String), String> {
    let Value::String(value) = input else {
        return Err(format!("{builtin} requires string inputs"));
    };
    let values = arguments[0].eval(input, env)?;
    match values.as_slice() {
        [Value::String(argument)] => Ok((value.clone(), argument.clone())),
        _ => Err(format!("{builtin} requires string inputs")),
    }
}
