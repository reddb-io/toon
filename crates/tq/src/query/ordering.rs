//! jq's total order over values, used by sorting, grouping and comparison.

use super::eval::parse_number;

/// The ordering two values sort by. Numbers that cannot be read fall back to
/// equality rather than failing, because a sort has no way to report an error.
pub(super) fn compare_key_json(
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> std::cmp::Ordering {
    compare_json_values(left, right).unwrap_or(std::cmp::Ordering::Equal)
}

pub(super) fn compare_json_values(
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> Result<std::cmp::Ordering, String> {
    let left_rank = json_rank(left);
    let right_rank = json_rank(right);
    if left_rank != right_rank {
        return Ok(left_rank.cmp(&right_rank));
    }

    match (left, right) {
        (serde_json::Value::Null, serde_json::Value::Null) => Ok(std::cmp::Ordering::Equal),
        (serde_json::Value::Bool(left), serde_json::Value::Bool(right)) => Ok(left.cmp(right)),
        (serde_json::Value::Number(left), serde_json::Value::Number(right)) => {
            parse_number(&left.to_string())?
                .partial_cmp(&parse_number(&right.to_string())?)
                .ok_or_else(|| "cannot compare numbers".to_owned())
        }
        (serde_json::Value::String(left), serde_json::Value::String(right)) => Ok(left.cmp(right)),
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            for (left, right) in left.iter().zip(right) {
                let ordering = compare_json_values(left, right)?;
                if !ordering.is_eq() {
                    return Ok(ordering);
                }
            }
            Ok(left.len().cmp(&right.len()))
        }
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            let mut left_entries = left.iter().collect::<Vec<_>>();
            let mut right_entries = right.iter().collect::<Vec<_>>();
            left_entries.sort_by_key(|(key, _)| *key);
            right_entries.sort_by_key(|(key, _)| *key);
            for ((left_key, left_value), (right_key, right_value)) in
                left_entries.iter().zip(&right_entries)
            {
                let key_ordering = left_key.cmp(right_key);
                if !key_ordering.is_eq() {
                    return Ok(key_ordering);
                }
                let value_ordering = compare_json_values(left_value, right_value)?;
                if !value_ordering.is_eq() {
                    return Ok(value_ordering);
                }
            }
            Ok(left_entries.len().cmp(&right_entries.len()))
        }
        _ => unreachable!("matching ranks have matching JSON variants"),
    }
}

fn json_rank(value: &serde_json::Value) -> u8 {
    match value {
        serde_json::Value::Null => 0,
        serde_json::Value::Bool(false) => 1,
        serde_json::Value::Bool(true) => 2,
        serde_json::Value::Number(_) => 3,
        serde_json::Value::String(_) => 4,
        serde_json::Value::Array(_) => 5,
        serde_json::Value::Object(_) => 6,
    }
}
