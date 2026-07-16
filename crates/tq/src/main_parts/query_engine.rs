fn evaluate(document: &Value, query: &str) -> Result<Vec<Value>, String> {
    Parser::new(query)?.parse()?.eval(document)
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Array(Vec<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Builtin(Builtin),
    Comma(Vec<Expr>),
    Field(Box<Expr>, String),
    Identity,
    Index(Box<Expr>, usize),
    Iter(Box<Expr>),
    Literal(Value),
    Object(Vec<(String, Expr)>),
    Pipe(Box<Expr>, Box<Expr>),
    Slice(Box<Expr>, Option<usize>, Option<usize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, PartialEq)]
enum Builtin {
    Add,
    FromEntries,
    GroupBy(Box<Expr>),
    Has(Box<Expr>),
    Join(Box<Expr>),
    Keys,
    Length,
    Map(Box<Expr>),
    MaxBy(Box<Expr>),
    MinBy(Box<Expr>),
    Select(Box<Expr>),
    SortBy(Box<Expr>),
    Split(Box<Expr>),
    Test(Box<Expr>),
    ToEntries,
    Unique,
}

impl Expr {
    fn eval(&self, input: &Value) -> Result<Vec<Value>, String> {
        match self {
            Self::Array(items) => {
                let mut values = Vec::new();
                for item in items {
                    values.extend(item.eval(input)?);
                }
                Ok(vec![Value::Array(Array::List(values))])
            }
            Self::Binary(operator, left, right) => {
                let left_values = left.eval(input)?;
                let right_values = right.eval(input)?;
                let mut output = Vec::new();
                for left_value in &left_values {
                    for right_value in &right_values {
                        output.push(evaluate_binary(*operator, left_value, right_value)?);
                    }
                }
                Ok(output)
            }
            Self::Builtin(builtin) => evaluate_builtin(builtin, input),
            Self::Comma(expressions) => {
                let mut output = Vec::new();
                for expression in expressions {
                    output.extend(expression.eval(input)?);
                }
                Ok(output)
            }
            Self::Field(base, key) => Ok(base
                .eval(input)?
                .into_iter()
                .map(|value| match value {
                    Value::Object(document) => document.get(key).cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
                .collect()),
            Self::Identity => Ok(vec![input.clone()]),
            Self::Index(base, index) => Ok(base
                .eval(input)?
                .into_iter()
                .map(|value| match value {
                    Value::Array(array) => array.get(*index).unwrap_or(Value::Null),
                    _ => Value::Null,
                })
                .collect()),
            Self::Iter(base) => Ok(base
                .eval(input)?
                .into_iter()
                .flat_map(|value| match value {
                    Value::Array(array) => array.values(),
                    _ => vec![Value::Null],
                })
                .collect()),
            Self::Literal(value) => Ok(vec![value.clone()]),
            Self::Object(fields) => evaluate_object(fields, input),
            Self::Pipe(left, right) => {
                let mut output = Vec::new();
                for value in left.eval(input)? {
                    output.extend(right.eval(&value)?);
                }
                Ok(output)
            }
            Self::Slice(base, start, end) => Ok(base
                .eval(input)?
                .into_iter()
                .map(|value| match value {
                    Value::Array(array) => Value::Array(array.slice(*start, *end)),
                    _ => Value::Null,
                })
                .collect()),
        }
    }
}

fn evaluate_object(fields: &[(String, Expr)], input: &Value) -> Result<Vec<Value>, String> {
    let mut objects = vec![serde_json::Map::new()];
    for (key, expression) in fields {
        let values = expression.eval(input)?;
        let mut next_objects = Vec::new();
        for object in objects {
            for value in &values {
                let mut next = object.clone();
                next.insert(key.clone(), value.to_json_value());
                next_objects.push(next);
            }
        }
        objects = next_objects;
    }

    Ok(objects
        .into_iter()
        .map(serde_json::Value::Object)
        .map(Value::from_json_value)
        .collect())
}

fn evaluate_builtin(builtin: &Builtin, input: &Value) -> Result<Vec<Value>, String> {
    match builtin {
        Builtin::Add => evaluate_add(input).map(|value| vec![value]),
        Builtin::FromEntries => evaluate_from_entries(input).map(|value| vec![value]),
        Builtin::GroupBy(filter) => evaluate_group_by(input, filter).map(|value| vec![value]),
        Builtin::Has(key_filter) => {
            let keys = key_filter.eval(input)?;
            keys.into_iter()
                .map(|key| evaluate_has(input, &key))
                .collect()
        }
        Builtin::Join(separator_filter) => {
            evaluate_join(input, &single_string_arg(separator_filter, input, "join")?)
                .map(|value| vec![value])
        }
        Builtin::Keys => evaluate_keys(input).map(|value| vec![value]),
        Builtin::Length => evaluate_length(input).map(|value| vec![value]),
        Builtin::Map(filter) => {
            let Value::Array(array) = input else {
                return Err("cannot iterate over non-array".to_owned());
            };
            let mut values = Vec::new();
            for value in array.values() {
                values.extend(filter.eval(&value)?);
            }
            Ok(vec![Value::Array(Array::List(values))])
        }
        Builtin::Select(filter) => {
            let mut values = Vec::new();
            for value in filter.eval(input)? {
                if is_truthy(&value) {
                    values.push(input.clone());
                }
            }
            Ok(values)
        }
        Builtin::MaxBy(filter) => evaluate_min_max_by(input, filter, true).map(|value| vec![value]),
        Builtin::MinBy(filter) => {
            evaluate_min_max_by(input, filter, false).map(|value| vec![value])
        }
        Builtin::SortBy(filter) => evaluate_sort_by(input, filter).map(|value| vec![value]),
        Builtin::Split(separator_filter) => {
            evaluate_split(input, &single_string_arg(separator_filter, input, "split")?)
                .map(|value| vec![value])
        }
        Builtin::Test(pattern_filter) => {
            evaluate_test(input, &single_string_arg(pattern_filter, input, "test")?)
                .map(|value| vec![value])
        }
        Builtin::ToEntries => evaluate_to_entries(input).map(|value| vec![value]),
        Builtin::Unique => evaluate_unique(input).map(|value| vec![value]),
    }
}

fn evaluate_add(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("add cannot be applied to this value".to_owned());
    };

    let mut values = array.values().into_iter();
    let Some(mut total) = values.next() else {
        return Ok(Value::Null);
    };
    for value in values {
        total = add_values(&total, &value)?;
    }
    Ok(total)
}

fn evaluate_sort_by(input: &Value, filter: &Expr) -> Result<Value, String> {
    let mut keyed = keyed_array_values(input, filter)?;
    keyed.sort_by(|left, right| compare_key_json(&left.0, &right.0));
    Ok(Value::Array(Array::List(
        keyed.into_iter().map(|(_, value)| value).collect(),
    )))
}

fn evaluate_group_by(input: &Value, filter: &Expr) -> Result<Value, String> {
    let mut keyed = keyed_array_values(input, filter)?;
    keyed.sort_by(|left, right| compare_key_json(&left.0, &right.0));

    let mut groups: Vec<Value> = Vec::new();
    let mut current_key: Option<serde_json::Value> = None;
    let mut current_values = Vec::new();
    for (key, value) in keyed {
        if current_key.as_ref().is_some_and(|current| *current != key) {
            groups.push(Value::Array(Array::List(std::mem::take(
                &mut current_values,
            ))));
        }
        current_key = Some(key);
        current_values.push(value);
    }
    if current_key.is_some() {
        groups.push(Value::Array(Array::List(current_values)));
    }

    Ok(Value::Array(Array::List(groups)))
}

fn evaluate_unique(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("unique cannot be applied to this value".to_owned());
    };

    let mut values = array.values();
    values.sort_by(|left, right| compare_key_json(&left.to_json_value(), &right.to_json_value()));
    values.dedup_by(|left, right| left.to_json_value() == right.to_json_value());
    Ok(Value::Array(Array::List(values)))
}

fn evaluate_min_max_by(input: &Value, filter: &Expr, max: bool) -> Result<Value, String> {
    let keyed = keyed_array_values(input, filter)?;
    let selected = if max {
        keyed
            .into_iter()
            .max_by(|left, right| compare_key_json(&left.0, &right.0))
    } else {
        keyed
            .into_iter()
            .min_by(|left, right| compare_key_json(&left.0, &right.0))
    };
    Ok(selected.map(|(_, value)| value).unwrap_or(Value::Null))
}

fn keyed_array_values(
    input: &Value,
    filter: &Expr,
) -> Result<Vec<(serde_json::Value, Value)>, String> {
    let Value::Array(array) = input else {
        return Err("cannot order non-array".to_owned());
    };

    array
        .values()
        .into_iter()
        .map(|value| {
            let key = sort_key(filter, &value)?;
            Ok((key, value))
        })
        .collect()
}

fn sort_key(filter: &Expr, input: &Value) -> Result<serde_json::Value, String> {
    let values = filter.eval(input)?;
    if values.len() == 1 {
        Ok(values
            .into_iter()
            .next()
            .expect("one sort key exists")
            .to_json_value())
    } else {
        Ok(serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| value.to_json_value())
                .collect(),
        ))
    }
}

fn compare_key_json(left: &serde_json::Value, right: &serde_json::Value) -> std::cmp::Ordering {
    compare_json_values(left, right).unwrap_or(std::cmp::Ordering::Equal)
}

fn evaluate_has(input: &Value, key: &Value) -> Result<Value, String> {
    match (input, key) {
        (Value::Array(array), Value::Number(index)) => {
            let index = parse_usize(index)?;
            Ok(Value::Bool(index < array.len()))
        }
        (Value::Object(document), Value::String(key)) => {
            Ok(Value::Bool(document.get(key).is_some()))
        }
        (Value::Object(document), Value::Number(key)) => {
            Ok(Value::Bool(document.get(key).is_some()))
        }
        _ => Err("has() cannot check this value".to_owned()),
    }
}

fn evaluate_to_entries(input: &Value) -> Result<Value, String> {
    let entries = match input {
        Value::Array(array) => array
            .values()
            .into_iter()
            .enumerate()
            .map(|(index, value)| entry_value(Value::Number(index.to_string()), value))
            .collect(),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            map.into_iter()
                .map(|(key, value)| entry_value(Value::String(key), Value::from_json_value(value)))
                .collect()
        }
        _ => return Err("to_entries cannot be applied to this value".to_owned()),
    };
    Ok(Value::Array(Array::List(entries)))
}

fn entry_value(key: Value, value: Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("key".to_owned(), key.to_json_value());
    object.insert("value".to_owned(), value.to_json_value());
    Value::from_json_value(serde_json::Value::Object(object))
}

fn evaluate_from_entries(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("from_entries cannot be applied to this value".to_owned());
    };

    let mut object = serde_json::Map::new();
    for entry in array.values() {
        let Value::Object(document) = entry else {
            return Err("from_entries expects object entries".to_owned());
        };
        let key = document
            .get("key")
            .or_else(|| document.get("Key"))
            .or_else(|| document.get("name"))
            .or_else(|| document.get("Name"))
            .ok_or_else(|| "from_entries entry missing key".to_owned())?;
        let value = document
            .get("value")
            .or_else(|| document.get("Value"))
            .ok_or_else(|| "from_entries entry missing value".to_owned())?;
        object.insert(entry_key_string(key)?, value.to_json_value());
    }

    Ok(Value::from_json_value(serde_json::Value::Object(object)))
}

fn entry_key_string(value: &Value) -> Result<String, String> {
    match value {
        Value::Number(value) | Value::String(value) => Ok(value.clone()),
        _ => Err("from_entries keys must be strings or numbers".to_owned()),
    }
}

fn evaluate_split(input: &Value, separator: &str) -> Result<Value, String> {
    let Value::String(value) = input else {
        return Err("split cannot be applied to this value".to_owned());
    };

    let values = if separator.is_empty() {
        value
            .chars()
            .map(|character| Value::String(character.to_string()))
            .collect()
    } else {
        value
            .split(separator)
            .map(|part| Value::String(part.to_owned()))
            .collect()
    };
    Ok(Value::Array(Array::List(values)))
}

fn evaluate_join(input: &Value, separator: &str) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("join cannot be applied to this value".to_owned());
    };

    let parts = array
        .values()
        .into_iter()
        .map(|value| match value {
            Value::Bool(value) => Ok(value.to_string()),
            Value::Null => Ok(String::new()),
            Value::Number(value) | Value::String(value) => Ok(value),
            _ => Err("join cannot stringify this value".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::String(parts.join(separator)))
}

fn evaluate_test(input: &Value, pattern: &str) -> Result<Value, String> {
    let Value::String(value) = input else {
        return Err("test cannot be applied to this value".to_owned());
    };
    let regex = regex::Regex::new(pattern).map_err(|error| format!("invalid regex: {error}"))?;
    Ok(Value::Bool(regex.is_match(value)))
}

fn single_string_arg(filter: &Expr, input: &Value, builtin: &str) -> Result<String, String> {
    let values = filter.eval(input)?;
    match values.as_slice() {
        [Value::String(value)] => Ok(value.clone()),
        [_] => Err(format!("{builtin} argument must be a string")),
        _ => Err(format!("{builtin} argument must produce one value")),
    }
}

fn evaluate_keys(input: &Value) -> Result<Value, String> {
    match input {
        Value::Array(array) => Ok(Value::Array(Array::List(
            (0..array.len())
                .map(|index| Value::Number(index.to_string()))
                .collect(),
        ))),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            Ok(Value::Array(Array::List(
                keys.into_iter().map(Value::String).collect(),
            )))
        }
        _ => Err("keys cannot be applied to this value".to_owned()),
    }
}

fn evaluate_length(input: &Value) -> Result<Value, String> {
    let length = match input {
        Value::Array(array) => array.len() as f64,
        Value::Null => 0.0,
        Value::Number(value) => parse_number(value)?.abs(),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            map.len() as f64
        }
        Value::String(value) => value.chars().count() as f64,
        Value::Bool(_) => return Err("boolean has no length".to_owned()),
    };
    number_value(length)
}

fn evaluate_binary(operator: BinaryOp, left: &Value, right: &Value) -> Result<Value, String> {
    match operator {
        BinaryOp::Add => add_values(left, right),
        BinaryOp::Subtract => subtract_values(left, right),
        BinaryOp::Multiply => number_value(parse_number_value(left)? * parse_number_value(right)?),
        BinaryOp::Divide => {
            let divisor = parse_number_value(right)?;
            if divisor == 0.0 {
                return Err("division by zero".to_owned());
            }
            number_value(parse_number_value(left)? / divisor)
        }
        BinaryOp::Equal => Ok(Value::Bool(left.to_json_value() == right.to_json_value())),
        BinaryOp::NotEqual => Ok(Value::Bool(left.to_json_value() != right.to_json_value())),
        BinaryOp::Less => Ok(Value::Bool(compare_values(left, right)?.is_lt())),
        BinaryOp::LessEqual => Ok(Value::Bool(!compare_values(left, right)?.is_gt())),
        BinaryOp::Greater => Ok(Value::Bool(compare_values(left, right)?.is_gt())),
        BinaryOp::GreaterEqual => Ok(Value::Bool(!compare_values(left, right)?.is_lt())),
    }
}

fn add_values(left: &Value, right: &Value) -> Result<Value, String> {
    match (left, right) {
        (Value::Null, value) | (value, Value::Null) => Ok(value.clone()),
        (Value::Number(left), Value::Number(right)) => {
            number_value(parse_number(left)? + parse_number(right)?)
        }
        (Value::String(left), Value::String(right)) => Ok(Value::String(format!("{left}{right}"))),
        (Value::Array(left), Value::Array(right)) => {
            let mut values = left.values();
            values.extend(right.values());
            Ok(Value::Array(Array::List(values)))
        }
        (Value::Object(_), Value::Object(_)) => {
            let serde_json::Value::Object(mut left) = left.to_json_value() else {
                unreachable!("object serializes as object");
            };
            let serde_json::Value::Object(right) = right.to_json_value() else {
                unreachable!("object serializes as object");
            };
            left.extend(right);
            Ok(Value::from_json_value(serde_json::Value::Object(left)))
        }
        _ => Err("cannot add these values".to_owned()),
    }
}

fn subtract_values(left: &Value, right: &Value) -> Result<Value, String> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            number_value(parse_number(left)? - parse_number(right)?)
        }
        (Value::Array(left), Value::Array(right)) => {
            let remove = right
                .values()
                .into_iter()
                .map(|value| value.to_json_value())
                .collect::<Vec<_>>();
            Ok(Value::Array(Array::List(
                left.values()
                    .into_iter()
                    .filter(|value| !remove.contains(&value.to_json_value()))
                    .collect(),
            )))
        }
        _ => Err("cannot subtract these values".to_owned()),
    }
}

fn compare_values(left: &Value, right: &Value) -> Result<std::cmp::Ordering, String> {
    let left_json = left.to_json_value();
    let right_json = right.to_json_value();
    compare_json_values(&left_json, &right_json)
}

fn compare_json_values(
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

fn is_truthy(value: &Value) -> bool {
    !matches!(value, Value::Bool(false) | Value::Null)
}

fn parse_number_value(value: &Value) -> Result<f64, String> {
    match value {
        Value::Number(value) => parse_number(value),
        _ => Err("expected number".to_owned()),
    }
}

fn parse_number(value: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|_| format!("invalid number `{value}`"))
}

fn parse_usize(value: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("invalid array index `{value}`"))
}

fn number_value(value: f64) -> Result<Value, String> {
    if !value.is_finite() {
        return Err("number is not finite".to_owned());
    }
    if value.fract() == 0.0 {
        Ok(Value::Number(format!("{value:.0}")))
    } else {
        serde_json::Number::from_f64(value)
            .map(|number| Value::Number(number.to_string()))
            .ok_or_else(|| "number is not finite".to_owned())
    }
}

