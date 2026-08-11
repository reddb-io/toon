use super::super::eval;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("from_entries", 0, eval::call_from_entries),
    Builtin::new("has", 1, eval::call_has),
    Builtin::new("keys", 0, eval::call_keys),
    Builtin::new("to_entries", 0, eval::call_to_entries),
];
