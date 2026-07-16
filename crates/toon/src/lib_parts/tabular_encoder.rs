#[derive(Debug, Clone, PartialEq, Eq)]
struct TabularShape {
    fields: Vec<HeaderFieldShape>,
    paths: Vec<ColumnPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeaderFieldShape {
    key: String,
    list_delimiter: Option<char>,
    fixed_len: Option<usize>,
    child_table: bool,
    children: Vec<HeaderFieldShape>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColumnPath {
    path: Vec<String>,
    list_delimiter: Option<char>,
    fixed_len: Option<usize>,
    child_fields: Vec<HeaderFieldShape>,
}

fn tabular_shape(
    values: &[Value],
    options: EncodeOptions,
    depth: usize,
) -> Result<Option<TabularShape>, EncodeError> {
    if let Some(shape) = matrix_shape(values, options) {
        return Ok(Some(shape));
    }
    let Some(fields) = object_shape(values, options, depth)? else {
        return Ok(None);
    };
    let mut paths = Vec::new();
    collect_leaf_paths(&fields, &mut Vec::new(), &mut paths);
    Ok(Some(TabularShape { fields, paths }))
}

fn keyed_map_shape(
    document: &Document,
    options: EncodeOptions,
    depth: usize,
) -> Result<Option<TabularShape>, EncodeError> {
    if !options.keyed_map_collapse || document.fields.len() < 2 {
        return Ok(None);
    }
    let values = document
        .fields
        .iter()
        .map(|field| field.value.clone())
        .collect::<Vec<_>>();
    tabular_shape(&values, options, depth)
}

fn object_shape(
    values: &[Value],
    options: EncodeOptions,
    depth: usize,
) -> Result<Option<Vec<HeaderFieldShape>>, EncodeError> {
    check_encode_depth(depth, options)?;
    let Some(Value::Object(first)) = values.first() else {
        return Ok(None);
    };
    if first.fields.is_empty() {
        return Ok(None);
    }
    let mut fields = first
        .fields
        .iter()
        .map(|field| HeaderFieldShape {
            key: field.key.clone(),
            list_delimiter: None,
            fixed_len: None,
            child_table: false,
            children: Vec::new(),
        })
        .collect::<Vec<_>>();

    for value in values {
        let Value::Object(document) = value else {
            return Ok(None);
        };
        if document.fields.len() != fields.len() {
            return Ok(None);
        }
        if fields
            .iter()
            .any(|field| document.get(&field.key).is_none())
        {
            return Ok(None);
        }
    }

    for field in &mut fields {
        let cells = values
            .iter()
            .map(|value| {
                let Value::Object(document) = value else {
                    unreachable!("shape check already matched objects");
                };
                document
                    .get(&field.key)
                    .expect("shape check already matched keys")
                    .clone()
            })
            .collect::<Vec<_>>();
        if cells.iter().all(Value::is_primitive) {
            continue;
        }
        if options.primitive_array_columns
            && cells.iter().all(|cell| match cell {
                Value::Array(array) => array.values().iter().all(Value::is_primitive),
                _ => false,
            })
        {
            field.list_delimiter = Some(';');
            continue;
        }
        if options.object_array_columns && cells.iter().all(|cell| matches!(cell, Value::Array(_)))
        {
            if let Some(fixed_len) = matrix_column_shape(&cells) {
                field.fixed_len = Some(fixed_len);
                continue;
            }
            let child_values = cells
                .iter()
                .flat_map(|cell| match cell {
                    Value::Array(array) => array.values().to_vec(),
                    _ => unreachable!("checked arrays"),
                })
                .collect::<Vec<_>>();
            if let Some(children) = object_shape(&child_values, options, depth + 1)? {
                field.children = children;
                field.child_table = true;
                continue;
            }
        }
        if !options.nested_tabular_headers {
            return Ok(None);
        }
        let Some(children) = object_shape(&cells, options, depth + 1)? else {
            return Ok(None);
        };
        field.children = children;
    }

    Ok(Some(fields))
}

fn matrix_shape(values: &[Value], options: EncodeOptions) -> Option<TabularShape> {
    if !options.object_array_columns {
        return None;
    }
    let fixed_len = matrix_column_shape(values)?;
    let fields = vec![HeaderFieldShape {
        key: "values".to_owned(),
        list_delimiter: None,
        fixed_len: Some(fixed_len),
        child_table: false,
        children: Vec::new(),
    }];
    let paths = vec![ColumnPath {
        path: Vec::new(),
        list_delimiter: None,
        fixed_len: Some(fixed_len),
        child_fields: Vec::new(),
    }];
    Some(TabularShape { fields, paths })
}

fn matrix_column_shape(values: &[Value]) -> Option<usize> {
    let first_len = match values.first()? {
        Value::Array(array) if !array.values().is_empty() => array.values().len(),
        _ => return None,
    };
    values
        .iter()
        .all(|value| match value {
            Value::Array(array) => {
                array.values().len() == first_len && array.values().iter().all(Value::is_primitive)
            }
            _ => false,
        })
        .then_some(first_len)
}

fn collect_leaf_paths(
    fields: &[HeaderFieldShape],
    prefix: &mut Vec<String>,
    paths: &mut Vec<ColumnPath>,
) {
    for field in fields {
        prefix.push(field.key.clone());
        if field.child_table {
            paths.push(ColumnPath {
                path: prefix.clone(),
                list_delimiter: field.list_delimiter,
                fixed_len: field.fixed_len,
                child_fields: field.children.clone(),
            });
        } else if let Some(fixed_len) = field.fixed_len {
            paths.push(ColumnPath {
                path: prefix.clone(),
                list_delimiter: None,
                fixed_len: Some(fixed_len),
                child_fields: Vec::new(),
            });
        } else if field.children.is_empty() {
            paths.push(ColumnPath {
                path: prefix.clone(),
                list_delimiter: field.list_delimiter,
                fixed_len: None,
                child_fields: Vec::new(),
            });
        } else {
            collect_leaf_paths(&field.children, prefix, paths);
        }
        prefix.pop();
    }
}

fn value_at_path<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut cursor = value;
    for segment in path {
        let Value::Object(document) = cursor else {
            return None;
        };
        cursor = document.get(segment)?;
    }
    Some(cursor)
}

fn primitive_text(value: &Value, delimiter: char) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_owned(),
        Value::Number(value) => canonical_number(value),
        Value::String(value) => canonical_string(value, delimiter),
        Value::Array(_) | Value::Object(_) => unreachable!("not a primitive"),
    }
}

fn column_text(
    value: &Value,
    column: &ColumnPath,
    active_delimiter: char,
    options: EncodeOptions,
    child_output: &mut String,
    child_depth: usize,
) -> String {
    if !column.child_fields.is_empty() {
        write_child_rows(
            child_output,
            value,
            &column.child_fields,
            options,
            child_depth,
        );
        let Value::Array(array) = value else {
            unreachable!("object_shape checked child-table values");
        };
        return array.values().len().to_string();
    }
    if column.fixed_len.is_some() {
        let Value::Array(array) = value else {
            unreachable!("object_shape checked fixed-width values");
        };
        return array
            .values()
            .iter()
            .map(|value| primitive_text(value, active_delimiter))
            .collect::<Vec<_>>()
            .join(&active_delimiter.to_string());
    }
    let Some(list_delimiter) = column.list_delimiter else {
        return primitive_text(value, active_delimiter);
    };
    let Value::Array(array) = value else {
        unreachable!("object_shape checked primitive-array column values");
    };
    array
        .values()
        .iter()
        .map(|value| primitive_list_item_text(value, active_delimiter, list_delimiter))
        .collect::<Vec<_>>()
        .join(&list_delimiter.to_string())
}

fn write_child_rows(
    output: &mut String,
    value: &Value,
    fields: &[HeaderFieldShape],
    options: EncodeOptions,
    depth: usize,
) {
    let Value::Array(array) = value else {
        unreachable!("object_shape checked child-table arrays");
    };
    let mut paths = Vec::new();
    collect_leaf_paths(fields, &mut Vec::new(), &mut paths);
    for child in array.values() {
        write_indent(output, depth);
        let mut nested_output = String::new();
        let cells = paths
            .iter()
            .map(|path| {
                let cell =
                    value_at_path(&child, &path.path).expect("object_shape checked child paths");
                column_text(
                    cell,
                    path,
                    options.delimiter,
                    options,
                    &mut nested_output,
                    depth + 1,
                )
            })
            .collect::<Vec<_>>();
        output.push_str(&cells.join(&options.delimiter.to_string()));
        output.push('\n');
        output.push_str(&nested_output);
    }
}

fn primitive_list_item_text(value: &Value, active_delimiter: char, list_delimiter: char) -> String {
    let Value::String(value) = value else {
        return primitive_text(value, active_delimiter);
    };
    if needs_quotes(value, active_delimiter) || value.contains(list_delimiter) {
        quote_string(value)
    } else {
        value.to_owned()
    }
}

fn canonical_key(value: &str) -> String {
    if is_bare_key(value) {
        value.to_owned()
    } else {
        quote_string(value)
    }
}

/// Unquoted keys must match `^[A-Za-z_][A-Za-z0-9_.]*$` (§7.3).
fn is_bare_key(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.'))
}

fn canonical_string(value: &str, delimiter: char) -> String {
    if needs_quotes(value, delimiter) {
        quote_string(value)
    } else {
        value.to_owned()
    }
}

/// The §7.2 quoting checklist.
fn needs_quotes(value: &str, delimiter: char) -> bool {
    value.is_empty()
        || value.trim() != value
        || matches!(value, "true" | "false" | "null")
        || is_numeric_like(value)
        || value.contains([':', '"', '\\', '[', ']', '{', '}'])
        || value.chars().any(|character| (character as u32) < 0x20)
        || value.contains(delimiter)
        || value.starts_with('-')
}

fn quote_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

// ---------------------------------------------------------------------------
// Lazy-row instrumentation
// ---------------------------------------------------------------------------

