use super::super::eval;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[Builtin::new("length", 0, eval::call_length)];
