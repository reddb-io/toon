use reddb_io_toon::{Array, Value};
use regex::{Regex, RegexBuilder};

use super::super::ast::Expr;
use super::super::eval::Env;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("capture", 1, call_capture),
    Builtin::new("capture", 2, call_capture),
    Builtin::new("gsub", 2, call_gsub),
    Builtin::new("gsub", 3, call_gsub),
    Builtin::new("match", 1, call_match),
    Builtin::new("match", 2, call_match),
    Builtin::new("scan", 1, call_scan),
    Builtin::new("scan", 2, call_scan),
    Builtin::new("split", 2, call_split),
    Builtin::new("splits", 1, call_splits),
    Builtin::new("splits", 2, call_splits),
    Builtin::new("sub", 2, call_sub),
    Builtin::new("sub", 3, call_sub),
    Builtin::new("test", 1, call_test),
    Builtin::new("test", 2, call_test),
];

fn call_gsub(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    substitute(arguments, input, env, true)
}

fn call_capture(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let value = string_input(input)?;
    let (regex, flags) = evaluated_regex_and_flags(arguments, input, env)?;
    let mut output = Vec::new();
    for captures in regex.captures_iter(value) {
        let matched = captures.get(0).expect("capture zero is the whole match");
        if flags.contains('n') && matched.is_empty() {
            continue;
        }
        let mut object = serde_json::Map::new();
        for (index, name) in regex.capture_names().enumerate().skip(1) {
            if let Some(name) = name {
                let captured = captures
                    .get(index)
                    .map_or(serde_json::Value::Null, |value| {
                        serde_json::Value::String(value.as_str().to_owned())
                    });
                object.insert(name.to_owned(), captured);
            }
        }
        output.push(Value::from_json_value(serde_json::Value::Object(object)));
        if !flags.contains('g') {
            break;
        }
    }
    Ok(output)
}

fn call_test(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let value = string_input(input)?;
    let (regex, flags) = evaluated_regex_and_flags(arguments, input, env)?;
    let matched = if flags.contains('n') {
        regex.find_iter(value).any(|matched| !matched.is_empty())
    } else {
        regex.is_match(value)
    };
    Ok(vec![Value::Bool(matched)])
}

fn call_match(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let value = string_input(input)?;
    let (regex, flags) = evaluated_regex_and_flags(arguments, input, env)?;
    let captures = regex.captures_iter(value);
    let mut output = Vec::new();
    for capture in captures {
        let matched = capture.get(0).expect("capture zero is the whole match");
        if flags.contains('n') && matched.is_empty() {
            continue;
        }
        output.push(match_object(value, &regex, &capture));
        if !flags.contains('g') {
            break;
        }
    }
    Ok(output)
}

fn call_scan(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let value = string_input(input)?;
    let (regex, flags) = evaluated_regex_and_flags(arguments, input, env)?;
    let mut output = Vec::new();
    for captures in regex.captures_iter(value) {
        let matched = captures.get(0).expect("capture zero is the whole match");
        if flags.contains('n') && matched.is_empty() {
            continue;
        }
        if captures.len() == 1 {
            output.push(Value::String(matched.as_str().to_owned()));
        } else {
            output.push(Value::Array(Array::List(
                (1..captures.len())
                    .map(|index| {
                        captures.get(index).map_or(Value::Null, |capture| {
                            Value::String(capture.as_str().to_owned())
                        })
                    })
                    .collect(),
            )));
        }
    }
    Ok(output)
}

fn call_splits(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let value = string_input(input)?;
    let (regex, flags) = evaluated_regex_and_flags(arguments, input, env)?;
    Ok(regex_split(value, &regex, flags.contains('n'))
        .into_iter()
        .map(Value::String)
        .collect())
}

fn call_split(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    call_splits(arguments, input, env).map(|parts| vec![Value::Array(Array::List(parts))])
}

fn call_sub(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    substitute(arguments, input, env, false)
}

fn substitute(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
    global: bool,
) -> Result<Vec<Value>, String> {
    let value = string_input(input)?;
    let (regex, flags) = evaluated_pattern(&arguments[0], arguments.get(2), input, env)?;
    let replace_all = global || flags.contains('g');
    let mut end = 0;
    let mut pieces = Vec::new();
    for captures in regex.captures_iter(value) {
        let matched = captures.get(0).expect("capture zero is the whole match");
        if flags.contains('n') && matched.is_empty() {
            continue;
        }
        let capture_value = named_capture_object(&regex, &captures);
        let replacements = arguments[1]
            .eval(&capture_value, env)?
            .into_iter()
            .map(|replacement| match replacement {
                Value::String(replacement) => Ok(replacement),
                _ => Err("string and replacement cannot be added".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if replacements.is_empty() {
            return Ok(Vec::new());
        }
        pieces.push((value[end..matched.start()].to_owned(), replacements));
        end = matched.end();
        if !replace_all {
            break;
        }
    }
    let tail = &value[end..];
    let output_count = pieces
        .iter()
        .map(|(_, replacements)| replacements.len())
        .max()
        .unwrap_or(1);
    let mut outputs = Vec::new();
    for index in 0..output_count {
        let mut output = String::new();
        for (literal, replacements) in &pieces {
            let replacement = if replacements.len() == 1 {
                &replacements[0]
            } else if let Some(replacement) = replacements.get(index) {
                replacement
            } else {
                continue;
            };
            output.push_str(literal);
            output.push_str(replacement);
        }
        output.push_str(tail);
        outputs.push(Value::String(output));
    }
    Ok(outputs)
}

fn named_capture_object(regex: &Regex, captures: &regex::Captures<'_>) -> Value {
    let mut object = serde_json::Map::new();
    for (index, name) in regex.capture_names().enumerate().skip(1) {
        if let Some(name) = name {
            let captured = captures
                .get(index)
                .map_or(serde_json::Value::Null, |value| {
                    serde_json::Value::String(value.as_str().to_owned())
                });
            object.insert(name.to_owned(), captured);
        }
    }
    Value::from_json_value(serde_json::Value::Object(object))
}

fn regex_split(value: &str, regex: &Regex, ignore_empty_matches: bool) -> Vec<String> {
    let mut output = Vec::new();
    let mut end = 0;
    for matched in regex.find_iter(value) {
        if ignore_empty_matches && matched.is_empty() {
            continue;
        }
        output.push(value[end..matched.start()].to_owned());
        end = matched.end();
    }
    output.push(value[end..].to_owned());
    output
}

fn evaluated_regex_and_flags(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<(Regex, String), String> {
    evaluated_pattern(&arguments[0], arguments.get(1), input, env)
}

fn evaluated_pattern(
    pattern: &Expr,
    flags: Option<&Expr>,
    input: &Value,
    env: &Env,
) -> Result<(Regex, String), String> {
    let pattern = string_argument(pattern, input, env, "regex")?;
    let flags = flags
        .map(|argument| string_argument(argument, input, env, "flags"))
        .transpose()?
        .unwrap_or_default();
    compile_regex(&pattern, &flags).map(|regex| (regex, flags))
}

fn match_object(value: &str, regex: &Regex, captures: &regex::Captures<'_>) -> Value {
    let matched = captures.get(0).expect("capture zero is the whole match");
    let mut object = serde_json::Map::new();
    object.insert(
        "offset".to_owned(),
        character_number(value, matched.start()),
    );
    object.insert(
        "length".to_owned(),
        serde_json::Value::from(matched.as_str().chars().count()),
    );
    object.insert(
        "string".to_owned(),
        serde_json::Value::String(matched.as_str().to_owned()),
    );

    let names = regex.capture_names().skip(1);
    let capture_values = names
        .enumerate()
        .map(|(index, name)| capture_object(value, captures.get(index + 1), name))
        .collect();
    object.insert(
        "captures".to_owned(),
        serde_json::Value::Array(capture_values),
    );
    Value::from_json_value(serde_json::Value::Object(object))
}

fn capture_object(
    value: &str,
    matched: Option<regex::Match<'_>>,
    name: Option<&str>,
) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    if let Some(matched) = matched {
        object.insert(
            "offset".to_owned(),
            character_number(value, matched.start()),
        );
        object.insert(
            "length".to_owned(),
            serde_json::Value::from(matched.as_str().chars().count()),
        );
        object.insert(
            "string".to_owned(),
            serde_json::Value::String(matched.as_str().to_owned()),
        );
    } else {
        object.insert("offset".to_owned(), serde_json::Value::from(-1));
        object.insert("string".to_owned(), serde_json::Value::Null);
        object.insert("length".to_owned(), serde_json::Value::from(0));
    }
    object.insert(
        "name".to_owned(),
        name.map_or(serde_json::Value::Null, |name| {
            serde_json::Value::String(name.to_owned())
        }),
    );
    serde_json::Value::Object(object)
}

fn character_number(value: &str, byte_offset: usize) -> serde_json::Value {
    serde_json::Value::from(value[..byte_offset].chars().count())
}

fn compile_regex(pattern: &str, flags: &str) -> Result<Regex, String> {
    let mut builder = RegexBuilder::new(pattern);
    for flag in flags.chars() {
        match flag {
            'g' | 'n' => {}
            'i' => {
                builder.case_insensitive(true);
            }
            // jq names these from the matched text's perspective: `m` lets
            // dot cross lines. Rust already gives `s` its whole-text anchor
            // semantics by default; `p` combines those two modes.
            'm' => {
                builder.dot_matches_new_line(true);
            }
            's' => {}
            'p' => {
                builder.dot_matches_new_line(true);
            }
            'x' => {
                builder.ignore_whitespace(true);
            }
            // Rust's engine is leftmost-first. Accept jq's longest flag; for
            // patterns whose alternatives agree it is byte-for-byte identical.
            'l' => {}
            _ => return Err(format!("{flags} is not a valid modifier string")),
        }
    }
    builder
        .build()
        .map_err(|error| format!("Regex failure: {error}"))
}

fn string_input(input: &Value) -> Result<&str, String> {
    match input {
        Value::String(value) => Ok(value),
        _ => Err("value cannot be matched, as it is not a string".to_owned()),
    }
}

fn string_argument(
    expression: &Expr,
    input: &Value,
    env: &Env,
    name: &str,
) -> Result<String, String> {
    match expression.eval(input, env)?.as_slice() {
        [Value::String(value)] => Ok(value.clone()),
        [_] => Err(format!("{name} is not a string")),
        _ => Err(format!("{name} must produce one value")),
    }
}
