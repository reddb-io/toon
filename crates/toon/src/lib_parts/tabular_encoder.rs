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

/// The only inputs tabular shape detection reads. The encoder hands it these
/// four values, so shape detection never depends on the encoder's own option
/// struct. Nested field groups are canonical, so there is no switch for them.
#[derive(Debug, Clone, Copy)]
struct ShapeOptions {
    primitive_array_columns: bool,
    object_array_columns: bool,
    delimiter: char,
    max_depth: usize,
}

fn tabular_shape(
    values: &[Value],
    options: ShapeOptions,
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


fn object_shape(
    values: &[Value],
    options: ShapeOptions,
    depth: usize,
) -> Result<Option<Vec<HeaderFieldShape>>, EncodeError> {
    check_encode_depth(depth, options.delimiter, options.max_depth)?;
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
        let Some(children) = object_shape(&cells, options, depth + 1)? else {
            return Ok(None);
        };
        field.children = children;
    }

    Ok(Some(fields))
}

fn matrix_shape(values: &[Value], options: ShapeOptions) -> Option<TabularShape> {
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

/// The §7.2 quoting checklist for the TOONL and cyclic row writers. A leading
/// `#` is quoted so the value never decodes as a comment line (v4.1). This path
/// trims via [`str::trim`], which also trims Unicode whitespace; the document
/// encoder uses the stricter ASCII-only rule in [`canonical_needs_quotes`].
fn needs_quotes(value: &str, delimiter: char) -> bool {
    value.is_empty()
        || value.trim() != value
        || matches!(value, "true" | "false" | "null")
        || is_numeric_like(value)
        || value.contains([':', '"', '\\', '[', ']', '{', '}'])
        || value.chars().any(|character| (character as u32) < 0x20)
        || value.contains(delimiter)
        || value.starts_with('-')
        || value.starts_with('#')
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

