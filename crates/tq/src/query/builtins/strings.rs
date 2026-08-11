use super::super::eval;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("join", 1, eval::call_join),
    Builtin::new("split", 1, eval::call_split),
];
