//! The assignment family, lowered the way jq lowers it.
//!
//! Every operator here is sugar over two things the path layer already
//! provides: the paths its left-hand side selects, and `setpath` writing at
//! one of them. `.a = 1` is `reduce path(.a) as $p (.; setpath($p; 1))`, and
//! `.a |= f` is jq's `_modify`, which additionally deletes a path whose update
//! produced nothing. Nothing here reaches into a container directly, so the
//! laziness and materialisation rules `paths.rs` documents hold unchanged.
//!
//! The left-hand paths are located once, against the document that entered the
//! assignment. Later writes therefore never move a path still to be visited,
//! and neither do deletions: `_modify` collects them and applies one
//! `delpaths` at the end, which is why `[1,2,3] | .[] |= empty` empties the
//! array instead of skipping every second element.

use reddb_io_toon::Value;

use super::ast::{AssignOp, BinaryOp, Expr};
use super::eval::{self, Env};
use super::paths;

/// What an operator writes at one selected path.
enum Update<'a> {
    /// `|=`: the right-hand filter, re-evaluated at every selected value.
    Filter(&'a Expr),
    /// `op=`: the operator applied to the current value and the one value the
    /// right-hand filter produced.
    Binary(BinaryOp, Value),
    /// `//=`: that same value, kept only where the current one is falsy.
    Alternative(Value),
}

pub(super) fn evaluate(
    operator: AssignOp,
    target: &Expr,
    source: &Expr,
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let selected = paths::locate(target, input, env)?
        .into_iter()
        .map(|located| located.path)
        .collect::<Vec<_>>();

    match operator {
        // `|=` runs its right-hand side at each selected value, so it reads
        // the document once and produces exactly one edited document.
        AssignOp::Update => modify(input, &selected, &Update::Filter(source), env).map(|v| vec![v]),
        // The rest evaluate their right-hand side against the whole input
        // first, as jq's `rhs as $x | …` lowering does. A generator there
        // produces one edited document per value it yields.
        AssignOp::Set => source
            .eval(input, env)?
            .iter()
            .map(|value| set_all(input, &selected, value))
            .collect(),
        AssignOp::Arithmetic(binary) => source
            .eval(input, env)?
            .into_iter()
            .map(|value| modify(input, &selected, &Update::Binary(binary, value), env))
            .collect(),
        AssignOp::Alternative => source
            .eval(input, env)?
            .into_iter()
            .map(|value| modify(input, &selected, &Update::Alternative(value), env))
            .collect(),
    }
}

/// jq's `_assign`: the same value written at every selected path.
fn set_all(input: &Value, selected: &[Vec<Value>], value: &Value) -> Result<Value, String> {
    let mut result = input.clone();
    for path in selected {
        result = paths::set_path(&result, path, value)?;
    }
    Ok(result)
}

/// jq's `_modify`: each selected path is read out of the document built so
/// far, updated, and written back. A path whose update produces nothing is
/// collected instead, and all of them are deleted together at the end.
fn modify(
    input: &Value,
    selected: &[Vec<Value>],
    update: &Update<'_>,
    env: &Env,
) -> Result<Value, String> {
    let mut result = input.clone();
    let mut deleted = Vec::new();
    for path in selected {
        let current = paths::get_path(&result, path)?;
        match update.apply(&current, env)? {
            Some(value) => result = paths::set_path(&result, path, &value)?,
            None => deleted.push(path.clone()),
        }
    }
    if deleted.is_empty() {
        return Ok(result);
    }
    paths::delete_all(&result, deleted)
}

impl Update<'_> {
    /// The value to write at one path, or `None` to delete it.
    fn apply(&self, current: &Value, env: &Env) -> Result<Option<Value>, String> {
        match self {
            // Only the first output is kept, matching the `label`/`break` pair
            // jq's `_modify` wraps its update in.
            Self::Filter(filter) => Ok(filter.eval(current, env)?.into_iter().next()),
            Self::Binary(operator, value) => {
                eval::evaluate_binary(*operator, current, value).map(Some)
            }
            Self::Alternative(value) => Ok(Some(if eval::is_truthy(current) {
                current.clone()
            } else {
                value.clone()
            })),
        }
    }
}
