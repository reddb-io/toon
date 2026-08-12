use reddb_io_toon::Value;

use super::ast::Expr;
use super::eval::Env;

pub(super) fn evaluate_index(
    base: &Expr,
    index: &Expr,
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    for value in base.eval(input, env)? {
        for key in index.eval(&value, env)? {
            output.push(index_value(&value, &key)?);
        }
    }
    Ok(output)
}

pub(super) fn evaluate_iteration(
    base: &Expr,
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    for value in base.eval(input, env)? {
        match value {
            Value::Array(array) => output.extend(array.values()),
            Value::Object(document) => output.extend(document.values().cloned()),
            value => return Err(format!("Cannot iterate over {}", value_kind(&value))),
        }
    }
    Ok(output)
}

pub(super) fn evaluate_slice(
    base: &Expr,
    start: Option<&Expr>,
    end: Option<&Expr>,
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    for value in base.eval(input, env)? {
        let starts = evaluate_bounds(start, &value, env)?;
        let ends = evaluate_bounds(end, &value, env)?;
        for start in &starts {
            for end in &ends {
                output.push(slice_value(&value, *start, *end)?);
            }
        }
    }
    Ok(output)
}

pub(super) fn index_value(input: &Value, key: &Value) -> Result<Value, String> {
    match (input, key) {
        (Value::Array(array), Value::Number(index)) => {
            let index = normalize_index(parse_number(index)?, array.len());
            Ok(index
                .and_then(|index| array.get(index))
                .unwrap_or(Value::Null))
        }
        (Value::Object(document), Value::String(key)) => {
            Ok(document.get(key).cloned().unwrap_or(Value::Null))
        }
        (Value::Object(_), Value::Number(_)) => Ok(Value::Null),
        (Value::Null, Value::Number(_) | Value::String(_)) => Ok(Value::Null),
        _ => Err(format!(
            "Cannot index {} with {}",
            value_kind(input),
            index_description(key)
        )),
    }
}

pub(super) fn evaluate_bounds(
    expression: Option<&Expr>,
    input: &Value,
    env: &Env,
) -> Result<Vec<Option<f64>>, String> {
    let Some(expression) = expression else {
        return Ok(vec![None]);
    };
    expression
        .eval(input, env)?
        .into_iter()
        .map(|value| match value {
            Value::Number(number) => parse_number(&number).map(Some),
            _ => Err("Array/string slice indices must be integers".to_owned()),
        })
        .collect()
}

pub(super) fn slice_value(
    input: &Value,
    start: Option<f64>,
    end: Option<f64>,
) -> Result<Value, String> {
    match input {
        Value::Array(array) => {
            let (start, end) = slice_bounds(array.len(), start, end);
            Ok(Value::Array(array.slice(Some(start), Some(end))))
        }
        Value::String(value) => {
            let characters = value.chars().collect::<Vec<_>>();
            let (start, end) = slice_bounds(characters.len(), start, end);
            Ok(Value::String(characters[start..end].iter().collect()))
        }
        Value::Null => Ok(Value::Null),
        value => Err(format!("Cannot index {} with object", value_kind(value))),
    }
}

fn normalize_index(index: f64, len: usize) -> Option<usize> {
    let index = index.trunc();
    let index = if index < 0.0 {
        len as f64 + index
    } else {
        index
    };
    (index >= 0.0 && index < len as f64).then_some(index as usize)
}

pub(super) fn slice_bounds(len: usize, start: Option<f64>, end: Option<f64>) -> (usize, usize) {
    let start = normalize_bound(start.unwrap_or(0.0), len);
    let end = normalize_bound(end.unwrap_or(len as f64), len).max(start);
    (start, end)
}

fn normalize_bound(bound: f64, len: usize) -> usize {
    let bound = bound.trunc();
    if bound < 0.0 {
        (len as f64 + bound).max(0.0) as usize
    } else {
        bound.min(len as f64) as usize
    }
}

fn parse_number(number: &str) -> Result<f64, String> {
    let number = number
        .parse::<f64>()
        .map_err(|_| format!("invalid array index `{number}`"))?;
    number
        .is_finite()
        .then_some(number)
        .ok_or_else(|| format!("invalid array index `{number}`"))
}

fn index_description(value: &Value) -> String {
    match value {
        Value::String(value) => format!(
            "string {}",
            serde_json::to_string(value).expect("strings always serialize")
        ),
        value => value_kind(value).to_owned(),
    }
}

pub(super) fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "array",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Number(_) => "number",
        Value::Object(_) => "object",
        Value::String(_) => "string",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(input: &str) -> Value {
        Value::from_json_str(input).expect("valid json literal")
    }

    #[test]
    fn indexing_null_and_number_keyed_objects_yields_null() {
        assert_eq!(index_value(&Value::Null, &json("0")).unwrap(), Value::Null);
        assert_eq!(
            index_value(&Value::Null, &json("\"k\"")).unwrap(),
            Value::Null
        );
        assert_eq!(
            index_value(&json("{\"a\":1}"), &json("0")).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn indexing_incompatible_kinds_names_both_sides() {
        let error = index_value(&json("true"), &json("0")).unwrap_err();
        assert_eq!(error, "Cannot index boolean with number");
        let error = index_value(&json("[1]"), &json("\"k\"")).unwrap_err();
        assert_eq!(error, "Cannot index array with string \"k\"");
        let error = index_value(&json("1"), &json("null")).unwrap_err();
        assert_eq!(error, "Cannot index number with null");
        let error = index_value(&json("\"s\""), &json("[1]")).unwrap_err();
        assert_eq!(error, "Cannot index string with array");
        let error = index_value(&json("{\"a\":1}"), &json("{}")).unwrap_err();
        assert_eq!(error, "Cannot index object with object");
    }

    #[test]
    fn slicing_null_passes_through_and_other_scalars_error() {
        assert_eq!(slice_value(&Value::Null, None, None).unwrap(), Value::Null);
        let error = slice_value(&json("true"), Some(0.0), None).unwrap_err();
        assert_eq!(error, "Cannot index boolean with object");
    }

    #[test]
    fn slice_bounds_reject_non_numbers() {
        let env = Env::default();
        let error =
            evaluate_bounds(Some(&Expr::Literal(json("\"x\""))), &Value::Null, &env).unwrap_err();
        assert_eq!(error, "Array/string slice indices must be integers");
    }
}
