use super::super::eval;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("add", 0, eval::call_add),
    Builtin::new("error", 0, eval::call_error),
    Builtin::new("error", 1, eval::call_error),
    Builtin::new("not", 0, eval::call_not),
    Builtin::new("select", 1, eval::call_select),
];
