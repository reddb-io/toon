use std::cmp::Ordering;

use reddb_io_toon::{Array, Value};

use super::super::ast::Expr;
use super::super::eval::{self, Env};
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("all", 0, call_all),
    Builtin::new("all", 1, call_all),
    Builtin::new("all", 2, call_all),
    Builtin::new("any", 0, call_any),
    Builtin::new("any", 1, call_any),
    Builtin::new("any", 2, call_any),
    Builtin::new("contains", 1, call_contains),
    Builtin::new("first", 0, call_first),
    Builtin::new("first", 1, call_first),
    Builtin::new("flatten", 0, call_flatten),
    Builtin::new("flatten", 1, call_flatten),
    Builtin::new("group_by", 1, eval::call_group_by),
    Builtin::new("index", 1, call_index),
    Builtin::new("indices", 1, call_indices),
    Builtin::new("inside", 1, call_inside),
    Builtin::new("last", 0, call_last),
    Builtin::new("last", 1, call_last),
    Builtin::new("limit", 2, call_limit),
    Builtin::new("map", 1, eval::call_map),
    Builtin::new("max", 0, call_max),
    Builtin::new("max_by", 1, eval::call_max_by),
    Builtin::new("min", 0, call_min),
    Builtin::new("min_by", 1, eval::call_min_by),
    Builtin::new("nth", 1, call_nth),
    Builtin::new("nth", 2, call_nth),
    Builtin::new("range", 1, call_range),
    Builtin::new("range", 2, call_range),
    Builtin::new("range", 3, call_range),
    Builtin::new("reverse", 0, call_reverse),
    Builtin::new("rindex", 1, call_rindex),
    Builtin::new("sort", 0, call_sort),
    Builtin::new("sort_by", 1, eval::call_sort_by),
    Builtin::new("transpose", 0, call_transpose),
    Builtin::new("unique", 0, eval::call_unique),
    Builtin::new("unique_by", 1, call_unique_by),
    Builtin::new("until", 2, call_until),
    Builtin::new("while", 2, call_while),
];

fn call_sort(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let mut values = array_values(input, "number cannot be sorted, as it is not an array")?;
    values.sort_by(compare_values);
    Ok(vec![Value::Array(Array::List(values))])
}

fn call_reverse(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let mut values = match input {
        Value::Array(array) => array.values(),
        _ => return Err(cannot_index_with_number(input)),
    };
    values.reverse();
    Ok(vec![Value::Array(Array::List(values))])
}

fn call_min(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    min_max(input, false).map(|value| vec![value])
}

fn call_max(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    min_max(input, true).map(|value| vec![value])
}

fn min_max(input: &Value, max: bool) -> Result<Value, String> {
    let values = array_values(input, "value cannot be iterated over")?;
    let selected = if max {
        values.into_iter().max_by(compare_values)
    } else {
        values.into_iter().min_by(compare_values)
    };
    Ok(selected.unwrap_or(Value::Null))
}

fn call_unique_by(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let values = array_values(input, "Cannot iterate over number")?;
    let mut keyed = values
        .into_iter()
        .map(|value| Ok((filter_key(&arguments[0], &value, env)?, value)))
        .collect::<Result<Vec<_>, String>>()?;
    keyed.sort_by(|left, right| compare_json(&left.0, &right.0));
    keyed.dedup_by(|left, right| left.0 == right.0);
    Ok(vec![Value::Array(Array::List(
        keyed.into_iter().map(|(_, value)| value).collect(),
    ))])
}

fn filter_key(filter: &Expr, input: &Value, env: &Env) -> Result<serde_json::Value, String> {
    let values = filter.eval(input, env)?;
    if let [value] = values.as_slice() {
        Ok(value.to_json_value())
    } else {
        Ok(serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| value.to_json_value())
                .collect(),
        ))
    }
}

fn call_flatten(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let depth = if arguments.is_empty() {
        usize::MAX
    } else {
        let depth = single_value(&arguments[0], input, env, "flatten depth")?;
        let number = as_number(&depth, "flatten depth must be a number")?;
        if number < 0.0 {
            return Err("flatten depth must not be negative".to_owned());
        }
        number.ceil() as usize
    };
    let values = array_values(input, "Cannot iterate over number")?;
    let mut flattened = Vec::new();
    flatten_values(values, depth, &mut flattened);
    Ok(vec![Value::Array(Array::List(flattened))])
}

fn flatten_values(values: Vec<Value>, depth: usize, output: &mut Vec<Value>) {
    for value in values {
        if depth > 0 {
            if let Value::Array(array) = value {
                flatten_values(array.values(), depth.saturating_sub(1), output);
                continue;
            }
        }
        output.push(value);
    }
}

fn call_range(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let numbers = arguments
        .iter()
        .map(|argument| {
            argument
                .eval(input, env)?
                .into_iter()
                .map(|value| as_number(&value, "Range bounds must be numeric"))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let starts = if arguments.len() == 1 {
        vec![0.0]
    } else {
        numbers[0].clone()
    };
    let ends = &numbers[usize::from(arguments.len() > 1)];
    let steps = if arguments.len() == 3 {
        numbers[2].clone()
    } else {
        vec![1.0]
    };
    let mut output = Vec::new();
    for start in starts {
        for end in ends {
            for step in &steps {
                append_range(start, *end, *step, &mut output)?;
            }
        }
    }
    Ok(output)
}

fn append_range(
    mut current: f64,
    end: f64,
    step: f64,
    output: &mut Vec<Value>,
) -> Result<(), String> {
    if step == 0.0 || (step > 0.0 && current >= end) || (step < 0.0 && current <= end) {
        return Ok(());
    }
    while (step > 0.0 && current < end) || (step < 0.0 && current > end) {
        output.push(number_value(current)?);
        current += step;
        if output.len() > 1_000_000 {
            return Err("range produced too many values for the eager evaluator".to_owned());
        }
    }
    Ok(())
}

fn call_first(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    if arguments.is_empty() {
        if matches!(input, Value::Null) {
            return Ok(vec![Value::Null]);
        }
        let values = match input {
            Value::Array(array) => array.values(),
            _ => return Err(cannot_index_with_number(input)),
        };
        return Ok(vec![values.first().cloned().unwrap_or(Value::Null)]);
    }
    Ok(arguments[0].eval(input, env)?.into_iter().take(1).collect())
}

fn call_last(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    if arguments.is_empty() {
        if matches!(input, Value::Null) {
            return Ok(vec![Value::Null]);
        }
        let values = match input {
            Value::Array(array) => array.values(),
            _ => return Err(cannot_index_with_number(input)),
        };
        return Ok(vec![values.last().cloned().unwrap_or(Value::Null)]);
    }
    Ok(arguments[0]
        .eval(input, env)?
        .into_iter()
        .last()
        .into_iter()
        .collect())
}

fn call_nth(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let index = single_value(&arguments[0], input, env, "nth index")?;
    let number = as_number(&index, "nth index must be a number")?;
    if arguments.len() == 1 {
        if matches!(input, Value::Null) {
            return Ok(vec![Value::Null]);
        }
        let values = match input {
            Value::Array(array) => array.values(),
            _ => return Err(cannot_index_with_number(input)),
        };
        let index = if number < 0.0 {
            values.len().checked_sub((-number).trunc() as usize)
        } else {
            Some(number.trunc() as usize)
        };
        return Ok(vec![index
            .and_then(|index| values.get(index).cloned())
            .unwrap_or(Value::Null)]);
    }
    if number < 0.0 {
        return Err("nth doesn't support negative indices".to_owned());
    }
    Ok(arguments[1]
        .eval(input, env)?
        .into_iter()
        .nth(number.ceil() as usize)
        .into_iter()
        .collect())
}

fn call_limit(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let count = single_value(&arguments[0], input, env, "limit count")?;
    let count = as_number(&count, "limit count must be numeric")?;
    let values = arguments[1].eval(input, env)?;
    if count < 0.0 {
        return Ok(values);
    }
    Ok(values.into_iter().take(count.ceil() as usize).collect())
}

fn call_until(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    iterate_condition(arguments, input, env, false)
}

fn call_while(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    iterate_condition(arguments, input, env, true)
}

fn iterate_condition(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
    emit_while_true: bool,
) -> Result<Vec<Value>, String> {
    let mut pending = vec![input.clone()];
    let mut output = Vec::new();
    let mut iterations = 0_usize;
    while let Some(value) = pending.pop() {
        let condition = arguments[0].eval(&value, env)?;
        let truthy = condition.iter().any(is_truthy);
        let continue_iterating = if emit_while_true { truthy } else { !truthy };
        if (emit_while_true && truthy) || (!emit_while_true && !continue_iterating) {
            output.push(value.clone());
        }
        if continue_iterating {
            let mut next = arguments[1].eval(&value, env)?;
            next.reverse();
            pending.extend(next);
        }
        iterations += 1;
        if iterations > 1_000_000 {
            return Err("iteration limit exceeded in eager evaluator".to_owned());
        }
    }
    Ok(output)
}

fn call_any(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    call_all_any(arguments, input, env, false)
}

fn call_all(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    call_all_any(arguments, input, env, true)
}

fn call_all_any(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
    all: bool,
) -> Result<Vec<Value>, String> {
    let generated = match arguments {
        [] | [_] => match input {
            Value::Array(array) => array.values(),
            _ => return Err(format!("Cannot iterate over {}", value_description(input))),
        },
        [generator, _] => generator.eval(input, env)?,
        _ => unreachable!("all/any arity is checked by dispatch"),
    };
    let mut booleans = Vec::new();
    for value in generated {
        if arguments.is_empty() {
            booleans.push(is_truthy(&value));
        } else {
            let condition = arguments.last().expect("condition exists");
            booleans.extend(condition.eval(&value, env)?.iter().map(is_truthy));
        }
    }
    let result = if all {
        booleans.into_iter().all(|value| value)
    } else {
        booleans.into_iter().any(|value| value)
    };
    Ok(vec![Value::Bool(result)])
}

fn call_contains(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    arguments[0]
        .eval(input, env)?
        .into_iter()
        .map(|value| contains_value(input, &value).map(Value::Bool))
        .collect()
}

fn call_inside(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    arguments[0]
        .eval(input, env)?
        .into_iter()
        .map(|value| contains_value(&value, input).map(Value::Bool))
        .collect()
}

fn contains_value(container: &Value, sought: &Value) -> Result<bool, String> {
    match (container, sought) {
        (Value::String(container), Value::String(sought)) => Ok(container.contains(sought)),
        (Value::Array(container), Value::Array(sought)) => {
            let available = container.values();
            for item in sought.values() {
                let mut found = false;
                for candidate in &available {
                    if contains_value(candidate, &item).unwrap_or(false) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Value::Object(container), Value::Object(sought)) => {
            let serde_json::Value::Object(sought) = sought.to_json_value() else {
                unreachable!("object serializes as object");
            };
            for (key, value) in sought {
                let Some(candidate) = container.get(&key) else {
                    return Ok(false);
                };
                if !contains_value(candidate, &Value::from_json_value(value))? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Value::Null, Value::Null)
        | (Value::Bool(_), Value::Bool(_))
        | (Value::Number(_), Value::Number(_)) => {
            Ok(container.to_json_value() == sought.to_json_value())
        }
        _ => Err("values cannot have their containment checked".to_owned()),
    }
}

fn call_indices(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    search_results(arguments, input, env, SearchResult::All)
}

fn call_index(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    search_results(arguments, input, env, SearchResult::First)
}

fn call_rindex(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    search_results(arguments, input, env, SearchResult::Last)
}

enum SearchResult {
    All,
    First,
    Last,
}

fn search_results(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
    result: SearchResult,
) -> Result<Vec<Value>, String> {
    arguments[0]
        .eval(input, env)?
        .into_iter()
        .map(|sought| {
            if matches!(input, Value::Null) {
                return Ok(Value::Null);
            }
            let indices = find_indices(input, &sought)?;
            Ok(match result {
                SearchResult::All => Value::Array(Array::List(
                    indices
                        .into_iter()
                        .map(|index| Value::Number(index.to_string()))
                        .collect(),
                )),
                SearchResult::First => indices
                    .first()
                    .map(|index| Value::Number(index.to_string()))
                    .unwrap_or(Value::Null),
                SearchResult::Last => indices
                    .last()
                    .map(|index| Value::Number(index.to_string()))
                    .unwrap_or(Value::Null),
            })
        })
        .collect()
}

fn find_indices(input: &Value, sought: &Value) -> Result<Vec<usize>, String> {
    match (input, sought) {
        (Value::String(input), Value::String(sought)) => {
            let input = input.chars().collect::<Vec<_>>();
            let sought = sought.chars().collect::<Vec<_>>();
            Ok(subslice_indices(&input, &sought))
        }
        (Value::Array(input), Value::Array(sought)) => {
            Ok(subslice_indices(&input.values(), &sought.values()))
        }
        (Value::Array(input), sought) => Ok(input
            .values()
            .iter()
            .enumerate()
            .filter_map(|(index, value)| (value == sought).then_some(index))
            .collect()),
        (Value::String(_), _) => Err("Cannot index string with number".to_owned()),
        _ => Err("Cannot index value".to_owned()),
    }
}

fn subslice_indices<T: PartialEq>(input: &[T], sought: &[T]) -> Vec<usize> {
    if sought.is_empty() || sought.len() > input.len() {
        return Vec::new();
    }
    input
        .windows(sought.len())
        .enumerate()
        .filter_map(|(index, window)| (window == sought).then_some(index))
        .collect()
}

fn call_transpose(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let rows = match input {
        Value::Array(array) => array.values(),
        _ => return Err(cannot_index_with_number(input)),
    };
    let rows = rows
        .into_iter()
        .map(|row| match row {
            Value::Array(array) => Ok(array.values()),
            value => Err(cannot_index_with_number(&value)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let columns = (0..width)
        .map(|column| {
            Value::Array(Array::List(
                rows.iter()
                    .map(|row| row.get(column).cloned().unwrap_or(Value::Null))
                    .collect(),
            ))
        })
        .collect();
    Ok(vec![Value::Array(Array::List(columns))])
}

fn array_values(input: &Value, error: &str) -> Result<Vec<Value>, String> {
    match input {
        Value::Array(array) => Ok(array.values()),
        _ => Err(error.to_owned()),
    }
}

fn single_value(filter: &Expr, input: &Value, env: &Env, name: &str) -> Result<Value, String> {
    let values = filter.eval(input, env)?;
    match values.as_slice() {
        [value] => Ok(value.clone()),
        _ => Err(format!("{name} must produce one value")),
    }
}

fn as_number(value: &Value, error: &str) -> Result<f64, String> {
    match value {
        Value::Number(value) => value.parse().map_err(|_| error.to_owned()),
        _ => Err(error.to_owned()),
    }
}

fn number_value(value: f64) -> Result<Value, String> {
    if !value.is_finite() {
        return Err("number is not finite".to_owned());
    }
    if value.fract() == 0.0 {
        Ok(Value::Number(format!("{value:.0}")))
    } else {
        serde_json::Number::from_f64(value)
            .map(|number| Value::Number(number.to_string()))
            .ok_or_else(|| "number is not finite".to_owned())
    }
}

fn is_truthy(value: &Value) -> bool {
    !matches!(value, Value::Bool(false) | Value::Null)
}

fn cannot_index_with_number(value: &Value) -> String {
    format!("Cannot index {} with number", value_description(value))
}

fn value_description(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "array",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Number(_) => "number",
        Value::Object(_) => "object",
        Value::String(_) => "string",
    }
}

fn compare_values(left: &Value, right: &Value) -> Ordering {
    compare_json(&left.to_json_value(), &right.to_json_value())
}

fn compare_json(left: &serde_json::Value, right: &serde_json::Value) -> Ordering {
    let rank = |value: &serde_json::Value| match value {
        serde_json::Value::Null => 0,
        serde_json::Value::Bool(false) => 1,
        serde_json::Value::Bool(true) => 2,
        serde_json::Value::Number(_) => 3,
        serde_json::Value::String(_) => 4,
        serde_json::Value::Array(_) => 5,
        serde_json::Value::Object(_) => 6,
    };
    match rank(left).cmp(&rank(right)) {
        Ordering::Equal => {}
        ordering => return ordering,
    }
    match (left, right) {
        (serde_json::Value::Null, serde_json::Value::Null) => Ordering::Equal,
        (serde_json::Value::Bool(left), serde_json::Value::Bool(right)) => left.cmp(right),
        (serde_json::Value::Number(left), serde_json::Value::Number(right)) => left
            .as_f64()
            .partial_cmp(&right.as_f64())
            .unwrap_or(Ordering::Equal),
        (serde_json::Value::String(left), serde_json::Value::String(right)) => left.cmp(right),
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            compare_slices(left, right)
        }
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            let mut left = left.iter().collect::<Vec<_>>();
            let mut right = right.iter().collect::<Vec<_>>();
            left.sort_by_key(|(key, _)| *key);
            right.sort_by_key(|(key, _)| *key);
            for ((left_key, left_value), (right_key, right_value)) in left.iter().zip(&right) {
                match left_key.cmp(right_key) {
                    Ordering::Equal => {}
                    ordering => return ordering,
                }
                match compare_json(left_value, right_value) {
                    Ordering::Equal => {}
                    ordering => return ordering,
                }
            }
            left.len().cmp(&right.len())
        }
        _ => Ordering::Equal,
    }
}

fn compare_slices(left: &[serde_json::Value], right: &[serde_json::Value]) -> Ordering {
    for (left, right) in left.iter().zip(right) {
        match compare_json(left, right) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    left.len().cmp(&right.len())
}
