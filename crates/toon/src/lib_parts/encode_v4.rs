// ---------------------------------------------------------------------------
// Canonical v4.1 encoder (issue #210)
// ---------------------------------------------------------------------------
//
// A faithful port of the TypeScript reference encoder (`encode/serialize.ts`,
// `encode/shape.ts`, `encode/replacer.ts`): keyed tabular form, recursive
// nested field groups, and v4.1 quoting are the canonical default here, with no
// opt-in flag. The output carries no trailing newline and round-trips through
// the v4 event decoder ([`decode_value_v4`]).

/// A path segment handed to an [`EncodeReplacer`], mirroring the TypeScript
/// `(string | number)[]`: object keys arrive as [`PathSegment::Key`] and array
/// indices as [`PathSegment::Index`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    Key(String),
    Index(usize),
}

/// A JSON-style replacer applied before shape detection and emission, mirroring
/// the second argument of `JSON.stringify`. It is called for the root (with an
/// empty key) and for every descendant. Returning [`None`] omits the entry (the
/// TypeScript `undefined`); at the root [`None`] keeps the original value.
pub type EncodeReplacer<'a> = dyn Fn(&str, &Value, &[PathSegment]) -> Option<Value> + 'a;

/// Options for [`encode_v4`]. Canonical v4.1 forms are unconditional; the
/// surviving wire-efficiency extensions remain explicit opt-ins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeV4Options {
    /// Active delimiter for array and tabular rows: comma, pipe, or tab.
    pub delimiter: char,
    /// Spaces per indentation level.
    pub indent_size: usize,
    /// Emit primitive-array columns in otherwise tabular object arrays.
    pub primitive_array_columns: bool,
    /// Emit recursive child tables and fixed-width matrix columns.
    pub object_array_columns: bool,
    /// Emit cyclic discriminated-array wire for repeated event streams.
    pub cyclic_discriminated_arrays: bool,
    /// Maximum nesting depth. `0` disables the guard for trusted input.
    pub max_depth: usize,
}

impl Default for EncodeV4Options {
    fn default() -> Self {
        Self {
            delimiter: DOCUMENT_DELIMITER,
            indent_size: DEFAULT_INDENT,
            primitive_array_columns: false,
            object_array_columns: false,
            cyclic_discriminated_arrays: false,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ResolvedV4 {
    delimiter: char,
    indent_size: usize,
    primitive_array_columns: bool,
    object_array_columns: bool,
    max_depth: usize,
}

/// Encodes a value using the canonical v4.1 forms.
pub fn encode_v4(value: &Value, options: EncodeV4Options) -> Result<String, EncodeError> {
    encode_v4_inner(value, options, None)
}

/// Encodes a value after applying a JSON-style replacer, mirroring the TS
/// `encode(input, { replacer })`.
pub fn encode_v4_with_replacer(
    value: &Value,
    options: EncodeV4Options,
    replacer: &EncodeReplacer,
) -> Result<String, EncodeError> {
    encode_v4_inner(value, options, Some(replacer))
}

fn encode_v4_inner(
    value: &Value,
    options: EncodeV4Options,
    replacer: Option<&EncodeReplacer>,
) -> Result<String, EncodeError> {
    validate_encode_delimiter(options.delimiter)?;
    let resolved = ResolvedV4 {
        delimiter: options.delimiter,
        indent_size: options.indent_size,
        primitive_array_columns: options.primitive_array_columns,
        object_array_columns: options.object_array_columns,
        max_depth: options.max_depth,
    };
    let value = match replacer {
        Some(replacer) => apply_replacer(value, replacer),
        None => value.clone(),
    };
    validate_v4_depth(&value, 0, options.max_depth)?;
    if options.cyclic_discriminated_arrays {
        if let Value::Object(document) = &value {
            let mut output = String::new();
            if write_cyclic_discriminated_arrays(
                &mut output,
                document,
                EncodeOptions {
                    cyclic_discriminated_arrays: true,
                    delimiter: options.delimiter,
                    max_depth: options.max_depth,
                    ..EncodeOptions::default()
                },
            )? {
                return Ok(output.trim_end_matches('\n').to_owned());
            }
        }
    }
    Ok(encode_v4_value(&value, resolved).join("\n"))
}

fn validate_v4_depth(value: &Value, depth: usize, max_depth: usize) -> Result<(), EncodeError> {
    if max_depth != 0 && depth > max_depth {
        return Err(EncodeError {
            message: "maximum nesting depth exceeded",
            max_depth: Some(max_depth),
        });
    }
    match value {
        Value::Object(document) => {
            for field in &document.fields {
                match &field.value {
                    Value::Object(nested) => {
                        validate_v4_depth(&Value::Object(nested.clone()), depth + 1, max_depth)?;
                    }
                    Value::Array(array) => {
                        validate_v4_depth(&Value::Array(array.clone()), depth, max_depth)?;
                    }
                    _ => {}
                }
            }
        }
        Value::Array(array) => {
            for item in array.values() {
                if !item.is_primitive() {
                    validate_v4_depth(&item, depth + 1, max_depth)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn encode_v4_value(value: &Value, options: ResolvedV4) -> Vec<String> {
    if value.is_primitive() {
        return vec![primitive_text_v4(value, options.delimiter)];
    }
    match value {
        Value::Array(array) => encode_v4_array(None, &array.values(), 0, options),
        Value::Object(document) => match keyed_shape_v4(document, 1, options) {
            Some(shape) => encode_v4_keyed(None, document, &shape, 0, options),
            None => encode_v4_object(document, 0, options),
        },
        _ => unreachable!("primitives handled above"),
    }
}

fn encode_v4_object(document: &Document, depth: usize, options: ResolvedV4) -> Vec<String> {
    document
        .fields
        .iter()
        .flat_map(|field| encode_v4_field(&field.key, &field.value, depth, options))
        .collect()
}

fn encode_v4_field(key: &str, value: &Value, depth: usize, options: ResolvedV4) -> Vec<String> {
    let prefix = indentation(depth, options) + &canonical_key(key);
    if value.is_primitive() {
        return vec![format!("{prefix}: {}", primitive_text_v4(value, options.delimiter))];
    }
    match value {
        Value::Array(array) => encode_v4_array(Some(key), &array.values(), depth, options),
        Value::Object(document) => {
            if let Some(shape) = keyed_shape_v4(document, depth + 1, options) {
                return encode_v4_keyed(Some(key), document, &shape, depth, options);
            }
            let mut lines = vec![format!("{prefix}:")];
            if !document.fields.is_empty() {
                lines.extend(encode_v4_object(document, depth + 1, options));
            }
            lines
        }
        _ => unreachable!("primitives handled above"),
    }
}

fn encode_v4_array(
    key: Option<&str>,
    values: &[Value],
    depth: usize,
    options: ResolvedV4,
) -> Vec<String> {
    let prefix = indentation(depth, options);
    if values.is_empty() {
        return vec![match key {
            None => format!("{prefix}[]"),
            Some(key) => format!("{prefix}{}: []", canonical_key(key)),
        }];
    }
    if values.iter().all(Value::is_primitive) {
        return vec![format!(
            "{prefix}{} {}",
            header(key, values.len(), None, options.delimiter, false),
            encode_cells(values, options.delimiter)
        )];
    }
    if let Some(shape) = tabular_shape_v4(values, depth + 1, options) {
        return encode_v4_tabular(key, values, &shape, depth, options);
    }
    let mut lines = vec![format!(
        "{prefix}{}",
        header(key, values.len(), None, options.delimiter, false)
    )];
    for item in values {
        lines.extend(encode_v4_list_item(item, depth + 1, options));
    }
    lines
}

fn encode_v4_tabular(
    key: Option<&str>,
    rows: &[Value],
    shape: &TabularShape,
    depth: usize,
    options: ResolvedV4,
) -> Vec<String> {
    let mut lines = vec![format!(
        "{}{}",
        indentation(depth, options),
        header(key, rows.len(), Some(&shape.fields), options.delimiter, false)
    )];
    for row in rows {
        let (cells, children) = encode_v4_tabular_row(row, &shape.paths, depth + 2, options);
        lines.push(format!("{}{}", indentation(depth + 1, options), cells));
        lines.extend(children);
    }
    lines
}

fn encode_v4_keyed(
    key: Option<&str>,
    document: &Document,
    shape: &TabularShape,
    depth: usize,
    options: ResolvedV4,
) -> Vec<String> {
    let mut lines = vec![format!(
        "{}{}",
        indentation(depth, options),
        header(key, document.fields.len(), Some(&shape.fields), options.delimiter, true)
    )];
    for field in &document.fields {
        let (cells, children) =
            encode_v4_tabular_row(&field.value, &shape.paths, depth + 2, options);
        lines.push(format!(
            "{}{}: {cells}",
            indentation(depth + 1, options),
            canonical_key(&field.key)
        ));
        lines.extend(children);
    }
    lines
}

fn encode_v4_list_item(value: &Value, depth: usize, options: ResolvedV4) -> Vec<String> {
    let prefix = format!("{}-", indentation(depth, options));
    if value.is_primitive() {
        return vec![format!("{prefix} {}", primitive_text_v4(value, options.delimiter))];
    }
    match value {
        Value::Array(array) => {
            let values = array.values();
            if values.is_empty() {
                return vec![format!(
                    "{prefix} {}",
                    header(None, 0, None, options.delimiter, false)
                )];
            }
            if values.iter().all(Value::is_primitive) {
                return vec![format!(
                    "{prefix} {} {}",
                    header(None, values.len(), None, options.delimiter, false),
                    encode_cells(&values, options.delimiter)
                )];
            }
            let mut lines = vec![format!(
                "{prefix} {}",
                header(None, values.len(), None, options.delimiter, false)
            )];
            for item in &values {
                lines.extend(encode_v4_list_item(item, depth + 1, options));
            }
            lines
        }
        Value::Object(document) => encode_v4_object_list_item(document, depth, options),
        _ => unreachable!("primitives handled above"),
    }
}

fn encode_v4_object_list_item(
    document: &Document,
    depth: usize,
    options: ResolvedV4,
) -> Vec<String> {
    let Some((first, rest)) = document.fields.split_first() else {
        return vec![format!("{}-", indentation(depth, options))];
    };

    let mut lines = if let Some(special) = encode_v4_first_container(&first.key, &first.value, depth, options) {
        special
    } else if first.value.is_primitive() {
        vec![format!(
            "{}- {}: {}",
            indentation(depth, options),
            canonical_key(&first.key),
            primitive_text_v4(&first.value, options.delimiter)
        )]
    } else if let Value::Array(array) = &first.value {
        let values = array.values();
        if values.is_empty() {
            vec![format!(
                "{}- {}: []",
                indentation(depth, options),
                canonical_key(&first.key)
            )]
        } else {
            let mut lines = vec![format!(
                "{}- {}",
                indentation(depth, options),
                header(Some(&first.key), values.len(), None, options.delimiter, false)
            )];
            for item in &values {
                lines.extend(encode_v4_list_item(item, depth + 2, options));
            }
            lines
        }
    } else {
        let Value::Object(nested) = &first.value else {
            unreachable!("primitives and arrays handled above");
        };
        let mut lines = vec![format!(
            "{}- {}:",
            indentation(depth, options),
            canonical_key(&first.key)
        )];
        if !nested.fields.is_empty() {
            lines.extend(encode_v4_object(nested, depth + 2, options));
        }
        lines
    };

    for field in rest {
        lines.extend(encode_v4_field(&field.key, &field.value, depth + 1, options));
    }
    lines
}

/// The first field of a list item may carry a tabular, primitive-array, or
/// keyed header on the hyphen line, with its body indented two levels deeper.
fn encode_v4_first_container(
    key: &str,
    value: &Value,
    depth: usize,
    options: ResolvedV4,
) -> Option<Vec<String>> {
    if let Value::Array(array) = value {
        let values = array.values();
        if !values.is_empty() && values.iter().all(Value::is_primitive) {
            return Some(vec![format!(
                "{}- {} {}",
                indentation(depth, options),
                header(Some(key), values.len(), None, options.delimiter, false),
                encode_cells(&values, options.delimiter)
            )]);
        }
        if let Some(shape) = tabular_shape_v4(&values, depth + 1, options) {
            let mut lines = vec![format!(
                "{}- {}",
                indentation(depth, options),
                header(
                    Some(key),
                    values.len(),
                    Some(&shape.fields),
                    options.delimiter,
                    false
                )
            )];
            for row in &values {
                let (cells, children) =
                    encode_v4_tabular_row(row, &shape.paths, depth + 3, options);
                lines.push(format!("{}{}", indentation(depth + 2, options), cells));
                lines.extend(children);
            }
            return Some(lines);
        }
    }
    if let Value::Object(document) = value {
        if let Some(shape) = keyed_shape_v4(document, depth + 1, options) {
            let mut lines = vec![format!(
                "{}- {}",
                indentation(depth, options),
                header(
                    Some(key),
                    document.fields.len(),
                    Some(&shape.fields),
                    options.delimiter,
                    true
                )
            )];
            for field in &document.fields {
                let (cells, children) =
                    encode_v4_tabular_row(&field.value, &shape.paths, depth + 3, options);
                lines.push(format!(
                    "{}{}: {cells}",
                    indentation(depth + 2, options),
                    canonical_key(&field.key)
                ));
                lines.extend(children);
            }
            return Some(lines);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Shape detection
// ---------------------------------------------------------------------------

fn tabular_shape_v4(
    values: &[Value],
    depth: usize,
    options: ResolvedV4,
) -> Option<TabularShape> {
    tabular_shape(values, shape_options_v4(options), depth)
        .expect("canonical depth was validated before shape detection")
}

fn keyed_shape_v4(
    document: &Document,
    depth: usize,
    options: ResolvedV4,
) -> Option<TabularShape> {
    if document.fields.len() < 2 {
        return None;
    }
    let rows = document
        .fields
        .iter()
        .map(|field| field.value.clone())
        .collect::<Vec<_>>();
    tabular_shape_v4(&rows, depth, options)
}

fn shape_options_v4(options: ResolvedV4) -> EncodeOptions {
    EncodeOptions {
        nested_tabular_headers: true,
        keyed_map_collapse: true,
        primitive_array_columns: options.primitive_array_columns,
        object_array_columns: options.object_array_columns,
        delimiter: options.delimiter,
        max_depth: options.max_depth,
        ..EncodeOptions::default()
    }
}

fn encode_v4_tabular_row(
    value: &Value,
    paths: &[ColumnPath],
    child_depth: usize,
    options: ResolvedV4,
) -> (String, Vec<String>) {
    let mut cells = Vec::new();
    let mut children = Vec::new();
    for path in paths {
        let cell = value_at_path(value, &path.path).expect("shape detection verified row paths");
        if !path.child_fields.is_empty() {
            let Value::Array(array) = cell else {
                unreachable!("shape detection verified child-table arrays");
            };
            cells.push(array.values().len().to_string());
            let mut child_paths = Vec::new();
            collect_leaf_paths(&path.child_fields, &mut Vec::new(), &mut child_paths);
            for child in array.values() {
                let (child_cells, descendants) =
                    encode_v4_tabular_row(&child, &child_paths, child_depth + 1, options);
                children.push(format!("{}{}", indentation(child_depth, options), child_cells));
                children.extend(descendants);
            }
        } else if path.fixed_len.is_some() {
            let Value::Array(array) = cell else {
                unreachable!("shape detection verified fixed-width arrays");
            };
            cells.extend(
                array
                    .values()
                    .iter()
                    .map(|item| primitive_text_v4(item, options.delimiter)),
            );
        } else if let Some(list_delimiter) = path.list_delimiter {
            let Value::Array(array) = cell else {
                unreachable!("shape detection verified primitive-array columns");
            };
            cells.push(
                array
                    .values()
                    .iter()
                    .map(|item| {
                        primitive_list_item_text_v4(item, options.delimiter, list_delimiter)
                    })
                    .collect::<Vec<_>>()
                    .join(&list_delimiter.to_string()),
            );
        } else {
            cells.push(primitive_text_v4(cell, options.delimiter));
        }
    };
    (cells.join(&options.delimiter.to_string()), children)
}

// ---------------------------------------------------------------------------
// Lexical helpers
// ---------------------------------------------------------------------------

fn header(
    key: Option<&str>,
    length: usize,
    fields: Option<&[HeaderFieldShape]>,
    delimiter: char,
    keyed: bool,
) -> String {
    let encoded_key = key.map(canonical_key).unwrap_or_default();
    let marker = if keyed { ":" } else { "" };
    let delimiter_marker = delimiter_prefix_text(delimiter);
    let field_text = fields.map_or(String::new(), |fields| {
        format!("{{{}}}", format_fields(fields, delimiter))
    });
    format!("{encoded_key}[{length}{marker}{delimiter_marker}]{field_text}:")
}

fn format_fields(fields: &[HeaderFieldShape], delimiter: char) -> String {
    fields
        .iter()
        .map(|field| {
            let name = canonical_key(&field.key);
            if let Some(list_delimiter) = field.list_delimiter {
                return format!("{name}[{list_delimiter}]");
            }
            if let Some(fixed_len) = field.fixed_len {
                return format!("{name}[{fixed_len}{}]", delimiter_prefix_text(delimiter));
            }
            if field.children.is_empty() {
                name
            } else {
                format!("{name}{{{}}}", format_fields(&field.children, delimiter))
            }
        })
        .collect::<Vec<_>>()
        .join(&delimiter.to_string())
}

fn primitive_list_item_text_v4(
    value: &Value,
    active_delimiter: char,
    list_delimiter: char,
) -> String {
    let Value::String(value) = value else {
        return primitive_text_v4(value, active_delimiter);
    };
    if needs_quotes_v4(value, active_delimiter) || value.contains(list_delimiter) {
        quote_string(value)
    } else {
        value.to_owned()
    }
}

fn encode_cells(values: &[Value], delimiter: char) -> String {
    values
        .iter()
        .map(|value| primitive_text_v4(value, delimiter))
        .collect::<Vec<_>>()
        .join(&delimiter.to_string())
}

/// Like [`primitive_text`], but strings use the canonical v4.1 quoting rule.
fn primitive_text_v4(value: &Value, delimiter: char) -> String {
    match value {
        Value::String(text) => canonical_string_v4(text, delimiter),
        _ => primitive_text(value, delimiter),
    }
}

fn canonical_string_v4(value: &str, delimiter: char) -> String {
    if needs_quotes_v4(value, delimiter) {
        quote_string(value)
    } else {
        value.to_owned()
    }
}

/// The §7.2 quoting checklist as the v4.1 reference encoder applies it:
/// whitespace is tested against ASCII space and tab only (the TS
/// `/^[ \t]|[ \t]$/`), so a value padded with non-ASCII whitespace such as
/// U+00A0 stays bare, and a leading `#` is quoted so it never reads as a comment.
fn needs_quotes_v4(value: &str, delimiter: char) -> bool {
    value.is_empty()
        || value.starts_with([' ', '\t'])
        || value.ends_with([' ', '\t'])
        || matches!(value, "true" | "false" | "null")
        || is_numeric_like(value)
        || value.contains([':', '"', '\\', '[', ']', '{', '}'])
        || value.chars().any(|character| (character as u32) < 0x20)
        || value.contains(delimiter)
        || value.starts_with('-')
        || value.starts_with('#')
}

fn indentation(depth: usize, options: ResolvedV4) -> String {
    " ".repeat(depth * options.indent_size)
}

// ---------------------------------------------------------------------------
// Replacer
// ---------------------------------------------------------------------------

/// Applies the JSON-style replacer before shape detection and emission,
/// mirroring the TypeScript `applyReplacer`.
fn apply_replacer(root: &Value, replacer: &EncodeReplacer) -> Value {
    match replacer("", root, &[]) {
        None => transform_children(root, replacer, &[]),
        Some(replaced) => transform_children(&replaced, replacer, &[]),
    }
}

fn transform_children(value: &Value, replacer: &EncodeReplacer, path: &[PathSegment]) -> Value {
    match value {
        Value::Array(array) => transform_array(&array.values(), replacer, path),
        Value::Object(document) => transform_object(document, replacer, path),
        _ => value.clone(),
    }
}

fn transform_object(document: &Document, replacer: &EncodeReplacer, path: &[PathSegment]) -> Value {
    let mut fields = Vec::new();
    for field in &document.fields {
        let mut child_path = path.to_vec();
        child_path.push(PathSegment::Key(field.key.clone()));
        if let Some(replaced) = replacer(&field.key, &field.value, &child_path) {
            fields.push(Field {
                key: field.key.clone(),
                value: transform_children(&replaced, replacer, &child_path),
            });
        }
    }
    Value::Object(Document { fields })
}

fn transform_array(values: &[Value], replacer: &EncodeReplacer, path: &[PathSegment]) -> Value {
    let mut result = Vec::new();
    for (index, item) in values.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(PathSegment::Index(index));
        if let Some(replaced) = replacer(&index.to_string(), item, &child_path) {
            result.push(transform_children(&replaced, replacer, &child_path));
        }
    }
    Value::Array(Array::List(result))
}
