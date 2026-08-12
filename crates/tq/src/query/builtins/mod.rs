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
}

impl Builtin {
    pub(super) const fn new(name: &'static str, arity: usize, call: BuiltinFn) -> Self {
        Self { name, arity, call }
    }
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
