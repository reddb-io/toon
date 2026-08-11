use super::super::eval;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[Builtin::new("test", 1, eval::call_test)];
