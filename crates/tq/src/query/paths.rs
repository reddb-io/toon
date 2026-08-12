//! The path layer: jq's dual-mode evaluation.
//!
//! Every filter has a value meaning, answered by `eval.rs`: what does it
//! produce? A path expression also has a location meaning, answered here:
//! where did each produced value live? Only the forms jq accepts inside
//! `path()` have one, so anything else reports jq's `Invalid path expression`
//! diagnostic rather than inventing a location for a computed value.
//!
//! Reads walk the same lazy accessors the value evaluator uses, so a field or
//! index query still decodes exactly the tabular rows it touches (ADR 0002).
//! Writes are that ADR's sanctioned exception: `setpath` and `delpaths`
//! materialise a touched tabular array into a list array, because a row-backed
//! array cannot represent an edited row. Untouched siblings stay lazy, so a
//! write next to a large table does not decode it.

use reddb_io_toon::{Array, Document, Value};

use super::ast::Expr;
use super::eval::{self, Env};
use super::indexing;

/// A value together with the path that reached it.
#[derive(Clone)]
pub(super) struct Located {
    pub(super) path: Vec<Value>,
    pub(super) value: Value,
}

impl Located {
    fn root(value: &Value) -> Self {
        Self {
            path: Vec::new(),
            value: value.clone(),
        }
    }

    fn descend(&self, component: Value, value: Value) -> Self {
        let mut path = self.path.clone();
        path.push(component);
        Self { path, value }
    }

    fn extend(&self, components: &[Value], value: Value) -> Self {
        let mut path = self.path.clone();
        path.extend(components.iter().cloned());
        Self { path, value }
    }
}

/// Locates every value `expression` selects out of `input`.
pub(super) fn locate(expression: &Expr, input: &Value, env: &Env) -> Result<Vec<Located>, String> {
    locate_from(expression, &Located::root(input), env)
}

/// The input and everything reachable below it, in jq's `..` order.
pub(super) fn descendants(input: &Value, env: &Env) -> Result<Vec<Located>, String> {
    locate_recurse(&Located::root(input), None, None, env)
}

fn locate_from(expression: &Expr, from: &Located, env: &Env) -> Result<Vec<Located>, String> {
    let _depth = env.enter()?;
    match expression {
        Expr::Identity => Ok(vec![from.clone()]),
        Expr::Empty => Ok(Vec::new()),
        Expr::Field(base, key) => {
            let mut output = Vec::new();
            for located in locate_from(base, from, env)? {
                let value = match &located.value {
                    Value::Object(document) => document.get(key).cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                };
                output.push(located.descend(Value::String(key.clone()), value));
            }
            Ok(output)
        }
        Expr::Index(base, index) => {
            let mut output = Vec::new();
            for located in locate_from(base, from, env)? {
                for key in index.eval(&located.value, env)? {
                    let value = indexing::index_value(&located.value, &key)?;
                    output.push(located.descend(key, value));
                }
            }
            Ok(output)
        }
        Expr::Slice(base, start, end) => {
            locate_slice(base, start.as_deref(), end.as_deref(), from, env)
        }
        Expr::Iter(base) => {
            let mut output = Vec::new();
            for located in locate_from(base, from, env)? {
                for (component, value) in members(&located.value)? {
                    output.push(located.descend(component, value));
                }
            }
            Ok(output)
        }
        Expr::Pipe(left, right) => {
            let mut output = Vec::new();
            for located in locate_from(left, from, env)? {
                output.extend(locate_from(right, &located, env)?);
            }
            Ok(output)
        }
        Expr::Comma(expressions) => {
            let mut output = Vec::new();
            for expression in expressions {
                output.extend(locate_from(expression, from, env)?);
            }
            Ok(output)
        }
        Expr::Optional(expression) => Ok(locate_from(expression, from, env).unwrap_or_default()),
        // A `catch` handler produces a value, never a location, so an error
        // that actually reaches one cannot be turned into a path.
        Expr::Try(expression, handler) => match locate_from(expression, from, env) {
            Ok(located) => Ok(located),
            Err(error) if handler.is_some() => Err(error),
            Err(_) => Ok(Vec::new()),
        },
        Expr::Conditional(branches, fallback) => locate_conditional(branches, fallback, from, env),
        Expr::Alternative(left, right) => {
            let located = locate_from(left, from, env)
                .unwrap_or_default()
                .into_iter()
                .filter(|located| eval::is_truthy(&located.value))
                .collect::<Vec<_>>();
            if located.is_empty() {
                locate_from(right, from, env)
            } else {
                Ok(located)
            }
        }
        Expr::Bind(source, pattern, body) => {
            let mut output = Vec::new();
            for value in source.eval(&from.value, env)? {
                output.extend(locate_from(body, from, &env.bind(pattern, &value)?)?);
            }
            Ok(output)
        }
        Expr::Def {
            name,
            parameters,
            body,
            rest,
        } => locate_from(rest, from, &env.define(name, parameters, body)),
        Expr::Call(name, arguments) => locate_call(expression, name, arguments, from, env),
        expression => invalid_path(expression, from, env),
    }
}

fn locate_slice(
    base: &Expr,
    start: Option<&Expr>,
    end: Option<&Expr>,
    from: &Located,
    env: &Env,
) -> Result<Vec<Located>, String> {
    let mut output = Vec::new();
    for located in locate_from(base, from, env)? {
        let starts = indexing::evaluate_bounds(start, &located.value, env)?;
        let ends = indexing::evaluate_bounds(end, &located.value, env)?;
        for start in &starts {
            for end in &ends {
                let value = indexing::slice_value(&located.value, *start, *end)?;
                output.push(located.descend(slice_component(*start, *end), value));
            }
        }
    }
    Ok(output)
}

fn locate_conditional(
    branches: &[(Expr, Expr)],
    fallback: &Expr,
    from: &Located,
    env: &Env,
) -> Result<Vec<Located>, String> {
    let Some(((condition, selected), remaining)) = branches.split_first() else {
        return locate_from(fallback, from, env);
    };

    let mut output = Vec::new();
    for value in condition.eval(&from.value, env)? {
        if eval::is_truthy(&value) {
            output.extend(locate_from(selected, from, env)?);
        } else {
            output.extend(locate_conditional(remaining, fallback, from, env)?);
        }
    }
    Ok(output)
}

/// The builtins that carry a location. A user definition is expanded first, so
/// a filter written with `def` is a path expression whenever its body is.
fn locate_call(
    expression: &Expr,
    name: &str,
    arguments: &[Expr],
    from: &Located,
    env: &Env,
) -> Result<Vec<Located>, String> {
    if let Some((body, scope)) = env.resolve_call(name, arguments) {
        return locate_from(&body, from, &scope);
    }

    match (name, arguments.len()) {
        ("select", 1) => {
            let mut output = Vec::new();
            for value in arguments[0].eval(&from.value, env)? {
                if eval::is_truthy(&value) {
                    output.push(from.clone());
                }
            }
            Ok(output)
        }
        ("getpath", 1) => {
            let mut output = Vec::new();
            for value in arguments[0].eval(&from.value, env)? {
                let components = components(&value)?;
                let value = get_path(&from.value, &components)?;
                output.push(from.extend(&components, value));
            }
            Ok(output)
        }
        ("recurse", 0) => locate_recurse(from, None, None, env),
        ("recurse", 1) => locate_recurse(from, Some(&arguments[0]), None, env),
        ("recurse", 2) => locate_recurse(from, Some(&arguments[0]), Some(&arguments[1]), env),
        _ => invalid_path(expression, from, env),
    }
}

fn locate_recurse(
    from: &Located,
    filter: Option<&Expr>,
    condition: Option<&Expr>,
    env: &Env,
) -> Result<Vec<Located>, String> {
    let _depth = env.enter()?;
    let mut output = vec![from.clone()];
    let children = match filter {
        Some(filter) => locate_from(filter, from, env)?,
        // Bare `recurse` is `recurse(.[]?)`, so a scalar simply has no children.
        None => members(&from.value)
            .unwrap_or_default()
            .into_iter()
            .map(|(component, value)| from.descend(component, value))
            .collect(),
    };
    for child in children {
        if !keeps(condition, &child.value, env)? {
            continue;
        }
        output.extend(locate_recurse(&child, filter, condition, env)?);
    }
    Ok(output)
}

fn keeps(condition: Option<&Expr>, value: &Value, env: &Env) -> Result<bool, String> {
    match condition {
        None => Ok(true),
        Some(condition) => Ok(condition.eval(value, env)?.iter().any(eval::is_truthy)),
    }
}

/// jq reports the value a non-path expression produced, so the diagnostic
/// names the computed result rather than the filter's text. A form that
/// produces nothing located nothing, which is not an error.
fn invalid_path(expression: &Expr, from: &Located, env: &Env) -> Result<Vec<Located>, String> {
    match expression.eval(&from.value, env)?.first() {
        None => Ok(Vec::new()),
        Some(value) => Err(format!(
            "Invalid path expression with result {}",
            compact(value)
        )),
    }
}

/// The addressable children of a container, keyed by their path component.
pub(super) fn members(value: &Value) -> Result<Vec<(Value, Value)>, String> {
    match value {
        Value::Array(array) => Ok((0..array.len())
            .filter_map(|index| {
                array
                    .get(index)
                    .map(|value| (Value::Number(index.to_string()), value))
            })
            .collect()),
        Value::Object(document) => Ok(document
            .entries()
            .map(|(key, value)| (Value::String(key.to_owned()), value.clone()))
            .collect()),
        value => Err(format!(
            "Cannot iterate over {}",
            indexing::value_kind(value)
        )),
    }
}

/// A path as jq spells it: an array of components.
pub(super) fn components(value: &Value) -> Result<Vec<Value>, String> {
    match value {
        Value::Array(array) => Ok(array.values()),
        _ => Err("Path must be specified as an array".to_owned()),
    }
}

pub(super) fn get_path(input: &Value, components: &[Value]) -> Result<Value, String> {
    let mut current = input.clone();
    for component in components {
        current = match slice_range(component) {
            Some((start, end)) => indexing::slice_value(&current, start, end)?,
            None => indexing::index_value(&current, component)?,
        };
    }
    Ok(current)
}

pub(super) fn set_path(
    input: &Value,
    components: &[Value],
    replacement: &Value,
) -> Result<Value, String> {
    let Some((component, rest)) = components.split_first() else {
        return Ok(replacement.clone());
    };
    if let Some((start, end)) = slice_range(component) {
        return set_slice(input, start, end, rest, replacement);
    }
    match component {
        Value::String(key) => set_field(input, key, rest, replacement),
        Value::Number(index) => set_element(input, index, rest, replacement),
        component => Err(format!("Invalid path component {}", compact(component))),
    }
}

fn set_field(
    input: &Value,
    key: &str,
    rest: &[Value],
    replacement: &Value,
) -> Result<Value, String> {
    let mut document = match input {
        Value::Object(document) => document.clone(),
        Value::Null => Document::default(),
        value => {
            return Err(format!(
                "Cannot index {} with string {}",
                indexing::value_kind(value),
                compact(&Value::String(key.to_owned()))
            ))
        }
    };
    let current = document.get(key).cloned().unwrap_or(Value::Null);
    document.set(key, set_path(&current, rest, replacement)?);
    Ok(Value::Object(document))
}

fn set_element(
    input: &Value,
    index: &str,
    rest: &[Value],
    replacement: &Value,
) -> Result<Value, String> {
    let mut values = match input {
        // The write that materialises a touched tabular array (ADR 0002).
        Value::Array(array) => array.values(),
        Value::Null => Vec::new(),
        value => {
            return Err(format!(
                "Cannot index {} with number",
                indexing::value_kind(value)
            ))
        }
    };
    let index = element_index(index, values.len())?;
    if index >= values.len() {
        values.resize(index + 1, Value::Null);
    }
    let current = values[index].clone();
    values[index] = set_path(&current, rest, replacement)?;
    Ok(Value::Array(Array::List(values)))
}

fn set_slice(
    input: &Value,
    start: Option<f64>,
    end: Option<f64>,
    rest: &[Value],
    replacement: &Value,
) -> Result<Value, String> {
    let mut values = match input {
        Value::Array(array) => array.values(),
        Value::Null => Vec::new(),
        value => {
            return Err(format!(
                "Cannot update field at object index of {}",
                indexing::value_kind(value)
            ))
        }
    };
    let (start, end) = indexing::slice_bounds(values.len(), start, end);
    let current = Value::Array(Array::List(values[start..end].to_vec()));
    let Value::Array(updated) = set_path(&current, rest, replacement)? else {
        return Err("A slice of an array can only be assigned another array".to_owned());
    };
    values.splice(start..end, updated.values());
    Ok(Value::Array(Array::List(values)))
}

pub(super) fn delete_path(input: &Value, components: &[Value]) -> Result<Value, String> {
    let Some((component, rest)) = components.split_first() else {
        return Ok(Value::Null);
    };
    if rest.is_empty() {
        return remove_component(input, component);
    }
    match input {
        Value::Null => Ok(Value::Null),
        Value::Array(_) | Value::Object(_) => {
            // A component the container does not address reaches nothing, so
            // the deletion below it is a no-op rather than an insertion.
            if !addresses(input, component) {
                return Ok(input.clone());
            }
            let child = match slice_range(component) {
                Some((start, end)) => indexing::slice_value(input, start, end)?,
                None => indexing::index_value(input, component)?,
            };
            let pruned = delete_path(&child, rest)?;
            set_path(input, std::slice::from_ref(component), &pruned)
        }
        value => Err(format!(
            "Cannot delete fields from {}",
            indexing::value_kind(value)
        )),
    }
}

/// Deletes every path, longest and last first, so an earlier removal never
/// shifts a later one out from under itself.
pub(super) fn delete_all(input: &Value, paths: Vec<Vec<Value>>) -> Result<Value, String> {
    let mut keyed = paths
        .into_iter()
        .map(|path| {
            let key = serde_json::Value::Array(path.iter().map(Value::to_json_value).collect());
            (key, path)
        })
        .collect::<Vec<_>>();
    keyed.sort_by(|left, right| eval::compare_key_json(&left.0, &right.0));
    keyed.dedup_by(|left, right| left.0 == right.0);

    let mut result = input.clone();
    for (_, path) in keyed.iter().rev() {
        result = delete_path(&result, path)?;
    }
    Ok(result)
}

fn remove_component(input: &Value, component: &Value) -> Result<Value, String> {
    if let Some((start, end)) = slice_range(component) {
        let Value::Array(array) = input else {
            return remove_from_non_container(input);
        };
        let mut values = array.values();
        let (start, end) = indexing::slice_bounds(values.len(), start, end);
        values.drain(start..end);
        return Ok(Value::Array(Array::List(values)));
    }

    match (input, component) {
        (Value::Object(document), Value::String(key)) => {
            let mut document = document.clone();
            document.remove(key);
            Ok(Value::Object(document))
        }
        (Value::Array(array), Value::Number(index)) => {
            let mut values = array.values();
            if let Some(index) = existing_index(index, values.len()) {
                values.remove(index);
            }
            Ok(Value::Array(Array::List(values)))
        }
        // A component of the wrong kind for this container addresses nothing
        // in it, matching tq's lenient index model.
        (Value::Array(_) | Value::Object(_), _) => Ok(input.clone()),
        (input, _) => remove_from_non_container(input),
    }
}

fn remove_from_non_container(input: &Value) -> Result<Value, String> {
    match input {
        Value::Null => Ok(Value::Null),
        value => Err(format!(
            "Cannot delete fields from {}",
            indexing::value_kind(value)
        )),
    }
}

fn addresses(input: &Value, component: &Value) -> bool {
    if slice_range(component).is_some() {
        return matches!(input, Value::Array(_));
    }
    match (input, component) {
        (Value::Object(document), Value::String(key)) => document.get(key).is_some(),
        (Value::Array(array), Value::Number(index)) => existing_index(index, array.len()).is_some(),
        _ => false,
    }
}

/// jq writes a slice path component as `{"start": …, "end": …}`, with `null`
/// standing for an open end.
fn slice_component(start: Option<f64>, end: Option<f64>) -> Value {
    let mut document = Document::default();
    document.set("start", bound_value(start));
    document.set("end", bound_value(end));
    Value::Object(document)
}

fn bound_value(bound: Option<f64>) -> Value {
    bound.map_or(Value::Null, |bound| {
        Value::Number(format!("{:.0}", bound.trunc()))
    })
}

fn slice_range(component: &Value) -> Option<(Option<f64>, Option<f64>)> {
    let Value::Object(document) = component else {
        return None;
    };
    let start = document.get("start")?;
    let end = document.get("end")?;
    Some((bound_number(start), bound_number(end)))
}

fn bound_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.parse().ok(),
        _ => None,
    }
}

/// An array index that already exists, normalizing jq's negative indices.
fn existing_index(index: &str, len: usize) -> Option<usize> {
    let index = index.parse::<f64>().ok()?.trunc();
    let index = if index < 0.0 {
        len as f64 + index
    } else {
        index
    };
    (index >= 0.0 && index < len as f64).then_some(index as usize)
}

/// jq grows an array to reach the index a write names. The ceiling turns a
/// wild index into a diagnostic instead of an allocation the host cannot make.
const MAX_ARRAY_GROWTH: f64 = 10_000_000.0;

fn element_index(index: &str, len: usize) -> Result<usize, String> {
    let index = index
        .parse::<f64>()
        .map_err(|_| format!("invalid array index `{index}`"))?
        .trunc();
    if index < 0.0 {
        let index = len as f64 + index;
        if index < 0.0 {
            return Err("Out of bounds negative array index".to_owned());
        }
        return Ok(index as usize);
    }
    if index > MAX_ARRAY_GROWTH {
        return Err("Array index is too large to grow the array to".to_owned());
    }
    Ok(index as usize)
}

pub(super) fn compact(value: &Value) -> String {
    serde_json::to_string(&value.to_json_value()).expect("tq values always serialize as JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(input: &str) -> Value {
        Value::from_json_str(input).expect("valid json literal")
    }

    #[test]
    fn writing_through_a_scalar_names_both_sides() {
        let error = set_path(&json("1"), &[json("\"a\"")], &json("2")).unwrap_err();
        assert!(
            error.starts_with("Cannot index number with string"),
            "{error}"
        );
        let error = set_path(&json("\"s\""), &[json("0")], &json("2")).unwrap_err();
        assert!(
            error.starts_with("Cannot index string with number"),
            "{error}"
        );
        let error =
            set_path(&json("1"), &[json("{\"start\":0,\"end\":1}")], &json("[]")).unwrap_err();
        assert!(
            error.starts_with("Cannot update field at object index"),
            "{error}"
        );
    }

    #[test]
    fn a_path_component_must_address_a_container() {
        let error = set_path(&Value::Null, &[json("true")], &json("1")).unwrap_err();
        assert!(error.starts_with("Invalid path component"), "{error}");
    }

    #[test]
    fn a_slice_can_only_be_assigned_an_array() {
        let error = set_path(
            &json("[1,2]"),
            &[json("{\"start\":0,\"end\":1}")],
            &json("9"),
        )
        .unwrap_err();
        assert!(error.starts_with("A slice of an array"), "{error}");
    }

    #[test]
    fn growing_an_array_beyond_the_ceiling_is_reported() {
        let error = set_path(&Value::Null, &[json("20000000")], &json("1")).unwrap_err();
        assert!(error.starts_with("Array index is too large"), "{error}");
        let error = element_index("x", 0).unwrap_err();
        assert!(error.starts_with("invalid array index"), "{error}");
    }

    #[test]
    fn deleting_through_a_scalar_is_reported() {
        let error = delete_path(&json("1"), &[json("\"a\"")]).unwrap_err();
        assert_eq!(error, "Cannot delete fields from number");
        let error = delete_path(&json("{\"a\":1}"), &[json("\"a\""), json("\"b\"")]).unwrap_err();
        assert_eq!(error, "Cannot delete fields from number");
        let error = delete_path(&json("1"), &[json("\"a\""), json("\"b\"")]).unwrap_err();
        assert_eq!(error, "Cannot delete fields from number");
        let error = delete_path(&json("1"), &[json("{\"start\":0,\"end\":1}")]).unwrap_err();
        assert_eq!(error, "Cannot delete fields from number");
    }

    #[test]
    fn deleting_what_a_component_cannot_address_is_a_no_op() {
        let input = json("{\"a\":1}");
        assert_eq!(delete_path(&input, &[json("0")]).unwrap(), input);
        assert_eq!(
            delete_path(&input, &[json("0"), json("\"b\"")]).unwrap(),
            input
        );
        assert_eq!(
            delete_path(&Value::Null, &[json("\"a\""), json("\"b\"")]).unwrap(),
            Value::Null
        );
        assert_eq!(
            delete_path(&json("[1]"), &[json("{\"start\":0,\"end\":1}"), json("0")]).unwrap(),
            json("[]")
        );
    }

    #[test]
    fn iterating_a_scalar_is_reported() {
        let error = members(&json("1")).unwrap_err();
        assert_eq!(error, "Cannot iterate over number");
    }

    #[test]
    fn a_path_is_an_array_of_components() {
        let error = components(&json("\"a\"")).unwrap_err();
        assert!(error.starts_with("Path must be specified as"), "{error}");
        assert_eq!(components(&json("[\"a\"]")).unwrap(), vec![json("\"a\"")]);
    }

    #[test]
    fn an_open_slice_component_keeps_jq_nulls() {
        assert_eq!(
            slice_component(None, Some(-2.0)),
            json("{\"start\":null,\"end\":-2}")
        );
        assert_eq!(slice_range(&json("{\"start\":0}")), None);
        assert_eq!(slice_range(&json("1")), None);
        assert_eq!(
            slice_range(&json("{\"start\":null,\"end\":2}")),
            Some((None, Some(2.0)))
        );
    }
}
