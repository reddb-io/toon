use super::super::eval;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("group_by", 1, eval::call_group_by),
    Builtin::new("map", 1, eval::call_map),
    Builtin::new("max_by", 1, eval::call_max_by),
    Builtin::new("min_by", 1, eval::call_min_by),
    Builtin::new("sort_by", 1, eval::call_sort_by),
    Builtin::new("unique", 0, eval::call_unique),
];
