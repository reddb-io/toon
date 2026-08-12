use reddb_io_toon::{Array, Value};

use super::super::ast::Expr;
use super::super::eval::{self, Env};
use super::super::paths;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("del", 1, call_del),
    Builtin::new("delpaths", 1, call_delpaths),
    Builtin::new("fromstream", 1, call_fromstream),
    Builtin::new("getpath", 1, call_getpath),
    Builtin::new("leaf_paths", 0, call_leaf_paths)
        .divergent("`leaf_paths/0` is jq 1.6 spelling that jq 1.7.1 removed"),
    Builtin::new("path", 1, call_path),
    Builtin::new("paths", 0, call_paths),
    Builtin::new("paths", 1, call_paths),
    Builtin::new("pick", 1, call_pick),
    Builtin::new("recurse", 0, call_recurse),
    Builtin::new("recurse", 1, call_recurse),
    Builtin::new("recurse", 2, call_recurse),
    Builtin::new("setpath", 2, call_setpath),
    Builtin::new("tostream", 0, call_tostream),
];

fn call_path(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    Ok(paths::locate(&arguments[0], input, env)?
        .into_iter()
        .map(|located| path_value(located.path))
        .collect())
}

/// `paths` and `paths(f)` walk everything below the root; the root itself has
/// the empty path, which jq excludes.
fn call_paths(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    for located in paths::descendants(input, env)? {
        if located.path.is_empty() {
            continue;
        }
        let keep = match arguments.first() {
            None => true,
            Some(filter) => filter
                .eval(&located.value, env)?
                .iter()
                .any(eval::is_truthy),
        };
        if keep {
            output.push(path_value(located.path));
        }
    }
    Ok(output)
}

fn call_leaf_paths(_: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    Ok(paths::descendants(input, env)?
        .into_iter()
        .filter(|located| {
            !located.path.is_empty() && !matches!(located.value, Value::Array(_) | Value::Object(_))
        })
        .map(|located| path_value(located.path))
        .collect())
}

fn call_getpath(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    arguments[0]
        .eval(input, env)?
        .iter()
        .map(|value| paths::get_path(input, &paths::components(value)?))
        .collect()
}

fn call_setpath(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    for path in arguments[0].eval(input, env)? {
        let components = paths::components(&path)?;
        for replacement in arguments[1].eval(input, env)? {
            output.push(paths::set_path(input, &components, &replacement)?);
        }
    }
    Ok(output)
}

fn call_delpaths(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    for value in arguments[0].eval(input, env)? {
        let requested = paths::components(&value)?
            .iter()
            .map(paths::components)
            .collect::<Result<Vec<_>, _>>()?;
        output.push(paths::delete_all(input, requested)?);
    }
    Ok(output)
}

fn call_del(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let requested = paths::locate(&arguments[0], input, env)?
        .into_iter()
        .map(|located| located.path)
        .collect();
    Ok(vec![paths::delete_all(input, requested)?])
}

/// jq's `pick`: rebuild a value that carries only the named paths, keeping the
/// original value at each one.
fn call_pick(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let mut picked = Value::Null;
    for located in paths::locate(&arguments[0], input, env)? {
        let value = paths::get_path(input, &located.path)?;
        picked = paths::set_path(&picked, &located.path, &value)?;
    }
    Ok(vec![picked])
}

fn call_recurse(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    recurse_values(input, arguments.first(), arguments.get(1), env, &mut output)?;
    Ok(output)
}

fn recurse_values(
    value: &Value,
    filter: Option<&Expr>,
    condition: Option<&Expr>,
    env: &Env,
    output: &mut Vec<Value>,
) -> Result<(), String> {
    let _depth = env.enter()?;
    output.push(value.clone());
    let children = match filter {
        Some(filter) => filter.eval(value, env)?,
        // Bare `recurse` is `recurse(.[]?)`, so a scalar has no children.
        None => paths::members(value)
            .unwrap_or_default()
            .into_iter()
            .map(|(_, child)| child)
            .collect(),
    };
    for child in children {
        match condition {
            None => recurse_values(&child, filter, condition, env, output)?,
            // `recurse(f; cond)` is `f | select(cond) | recurse`, so a
            // condition that produces several truthy values repeats the child.
            Some(kept_by) => {
                for kept in kept_by.eval(&child, env)? {
                    if eval::is_truthy(&kept) {
                        recurse_values(&child, filter, condition, env, output)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn call_tostream(_: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    stream_events(&[], input, env, &mut output)?;
    Ok(output)
}

/// jq's streamed form: a `[path, leaf]` event for every leaf, and a `[path]`
/// event closing each container after its last child.
fn stream_events(
    path: &[Value],
    value: &Value,
    env: &Env,
    output: &mut Vec<Value>,
) -> Result<(), String> {
    let _depth = env.enter()?;
    let children = paths::members(value).unwrap_or_default();
    let Some(last) = children.last().map(|(component, _)| component.clone()) else {
        output.push(event(vec![path_value(path.to_vec()), value.clone()]));
        return Ok(());
    };

    for (component, child) in &children {
        let mut child_path = path.to_vec();
        child_path.push(component.clone());
        stream_events(&child_path, child, env, output)?;
    }
    let mut closing = path.to_vec();
    closing.push(last);
    output.push(event(vec![path_value(closing)]));
    Ok(())
}

fn call_fromstream(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    let mut accumulated = Value::Null;
    for value in arguments[0].eval(input, env)? {
        let Value::Array(items) = &value else {
            return Err("Invalid streaming format".to_owned());
        };
        let items = items.values();
        let Some(path) = items.first() else {
            return Err("Invalid streaming format".to_owned());
        };
        let path = paths::components(path)?;

        match items.get(1) {
            // A leaf at the root is a whole value; anything deeper accumulates.
            Some(leaf) if path.is_empty() => output.push(leaf.clone()),
            Some(leaf) => accumulated = paths::set_path(&accumulated, &path, leaf)?,
            // Only the outermost container's closing event completes a value.
            None if path.len() == 1 => {
                output.push(std::mem::replace(&mut accumulated, Value::Null));
            }
            None => {}
        }
    }
    Ok(output)
}

fn event(items: Vec<Value>) -> Value {
    Value::Array(Array::List(items))
}

fn path_value(path: Vec<Value>) -> Value {
    Value::Array(Array::List(path))
}
