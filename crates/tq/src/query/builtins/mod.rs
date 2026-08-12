use reddb_io_toon::Value;

use super::ast::Expr;
use super::eval::Env;

mod arrays;
mod core;
mod formats;
mod math;
mod misc;
mod objects;
mod paths;
mod regex;
mod strings;
mod time;
mod types;

pub(super) type BuiltinFn = fn(&[Expr], &Value, &Env) -> Result<Vec<Value>, String>;

pub(super) struct Builtin {
    name: &'static str,
    arity: usize,
    call: BuiltinFn,
    /// The divergence ledger's summary, when this builtin is one jq 1.7.1
    /// answers differently or does not define at all. The compatibility
    /// classifier reads it from this table, so a divergent builtin cannot be
    /// registered without the classifier learning about it.
    divergence: Option<&'static str>,
}

impl Builtin {
    pub(super) const fn new(name: &'static str, arity: usize, call: BuiltinFn) -> Self {
        Self {
            name,
            arity,
            call,
            divergence: None,
        }
    }

    /// Marks this builtin as a row of the divergence ledger in
    /// `docs/tq-jq-parity.md`. `reason` is what a negative compatibility
    /// decision reports.
    pub(super) const fn divergent(self, reason: &'static str) -> Self {
        Self {
            divergence: Some(reason),
            ..self
        }
    }
}

/// What the evaluator's registry can say about one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Support {
    /// Dispatched, and pinned to jq 1.7.1 by the parity corpus.
    Compatible,
    /// Dispatched, but the divergence ledger records the difference.
    Divergent(&'static str),
    /// Not dispatched at this arity.
    Unknown,
}

const TABLES: &[&[Builtin]] = &[
    core::BUILTINS,
    arrays::BUILTINS,
    objects::BUILTINS,
    strings::BUILTINS,
    regex::BUILTINS,
    math::BUILTINS,
    types::BUILTINS,
    paths::BUILTINS,
    time::BUILTINS,
    formats::BUILTINS,
    misc::BUILTINS,
];

pub(super) fn supports(name: &str, arity: usize) -> bool {
    lookup(name, arity).is_some()
}

/// The registry's own verdict on a call, for the compatibility classifier.
pub(super) fn classify(name: &str, arity: usize) -> Support {
    match lookup(name, arity) {
        Some(builtin) => match builtin.divergence {
            Some(reason) => Support::Divergent(reason),
            None => Support::Compatible,
        },
        None => Support::Unknown,
    }
}

pub(super) fn evaluate(
    name: &str,
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    if name == "range" && arguments.is_empty() {
        return Err("unsupported identifier `range`".to_owned());
    }
    if let Some(builtin) = lookup(name, arguments.len()) {
        return (builtin.call)(arguments, input, env);
    }
    if arguments.is_empty()
        && TABLES
            .iter()
            .flat_map(|table| table.iter())
            .any(|builtin| builtin.name == name)
    {
        return Err("expected `LParen`, got `None`".to_owned());
    }
    Err(format!("unsupported identifier `{name}`"))
}

fn lookup(name: &str, arity: usize) -> Option<&'static Builtin> {
    TABLES
        .iter()
        .flat_map(|table| table.iter())
        .find(|builtin| builtin.name == name && builtin.arity == arity)
}
