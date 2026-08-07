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
    /// Spaces per indentation level; clamped to at least one.
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
}

/// A node of the recursive uniform-object shape shared by tabular and keyed
/// headers. A leaf carries no children; a group carries the nested field list.
#[derive(Debug, Clone)]
struct FieldNodeV4 {
    name: String,
    children: Option<Vec<FieldNodeV4>>,
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
        indent_size: options.indent_size.max(1),
    };
    let value = match replacer {
        Some(replacer) => apply_replacer(value, replacer),
        None => value.clone(),
    };
    validate_v4_depth(&value, 0, options.max_depth)?;
    if let Some(extension) = encode_v4_extension(&value, options)? {
        return Ok(extension);
    }
    Ok(encode_v4_value(&value, resolved).join("\n"))
}

/// The extension emitters are shared with the legacy surface. Comparing their
/// result with the same emitter's extension-free result distinguishes a real
/// extension wire from an ineligible fallback before returning to v4.1.
fn encode_v4_extension(
    value: &Value,
    options: EncodeV4Options,
) -> Result<Option<String>, EncodeError> {
    let base = EncodeOptions {
        delimiter: options.delimiter,
        max_depth: options.max_depth,
        ..EncodeOptions::default()
    };
    if options.cyclic_discriminated_arrays {
        let encoded = value.try_to_toon_with_options(EncodeOptions {
            cyclic_discriminated_arrays: true,
            ..base
        })?;
        if encoded != value.try_to_toon_with_options(base)? {
            return Ok(Some(encoded.trim_end_matches('\n').to_owned()));
        }
    }
    if options.primitive_array_columns || options.object_array_columns {
        let encoded = value.try_to_toon_with_options(EncodeOptions {
            primitive_array_columns: options.primitive_array_columns,
            object_array_columns: options.object_array_columns,
            ..base
        })?;
        if encoded != value.try_to_toon_with_options(base)? {
            return Ok(Some(encoded.trim_end_matches('\n').to_owned()));
        }
    }
    Ok(None)
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
        Value::Object(document) => match keyed_fields_v4(document) {
            Some(fields) => encode_v4_keyed(None, document, &fields, 0, options),
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
            if let Some(fields) = keyed_fields_v4(document) {
                return encode_v4_keyed(Some(key), document, &fields, depth, options);
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
    if values.iter().all(is_plain_object) {
        if let Some(fields) = tabular_fields_v4(values) {
            return encode_v4_tabular(key, values, &fields, depth, options);
        }
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
    fields: &[FieldNodeV4],
    depth: usize,
    options: ResolvedV4,
) -> Vec<String> {
    let mut lines = vec![format!(
        "{}{}",
        indentation(depth, options),
        header(key, rows.len(), Some(fields), options.delimiter, false)
    )];
    for row in rows {
        lines.push(format!(
            "{}{}",
            indentation(depth + 1, options),
            encode_cells(&collect_leaves_v4(row, fields), options.delimiter)
        ));
    }
    lines
}

fn encode_v4_keyed(
    key: Option<&str>,
    document: &Document,
    fields: &[FieldNodeV4],
    depth: usize,
    options: ResolvedV4,
) -> Vec<String> {
    let mut lines = vec![format!(
        "{}{}",
        indentation(depth, options),
        header(key, document.fields.len(), Some(fields), options.delimiter, true)
    )];
    for field in &document.fields {
        lines.push(format!(
            "{}{}: {}",
            indentation(depth + 1, options),
            canonical_key(&field.key),
            encode_cells(&collect_leaves_v4(&field.value, fields), options.delimiter)
        ));
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
        if values.iter().all(is_plain_object) {
            if let Some(fields) = tabular_fields_v4(&values) {
                let mut lines = vec![format!(
                    "{}- {}",
                    indentation(depth, options),
                    header(Some(key), values.len(), Some(&fields), options.delimiter, false)
                )];
                for row in &values {
                    lines.push(format!(
                        "{}{}",
                        indentation(depth + 2, options),
                        encode_cells(&collect_leaves_v4(row, &fields), options.delimiter)
                    ));
                }
                return Some(lines);
            }
        }
    }
    if let Value::Object(document) = value {
        if let Some(fields) = keyed_fields_v4(document) {
            let mut lines = vec![format!(
                "{}- {}",
                indentation(depth, options),
                header(Some(key), document.fields.len(), Some(&fields), options.delimiter, true)
            )];
            for field in &document.fields {
                lines.push(format!(
                    "{}{}: {}",
                    indentation(depth + 2, options),
                    canonical_key(&field.key),
                    encode_cells(&collect_leaves_v4(&field.value, &fields), options.delimiter)
                ));
            }
            return Some(lines);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Shape detection
// ---------------------------------------------------------------------------

fn is_plain_object(value: &Value) -> bool {
    matches!(value, Value::Object(_))
}

/// The recursive uniform-object shape required by v4.1 tabular form. Every row
/// must be a plain object with the same key set; a column is a leaf when all its
/// values are primitive, a nested group when all are non-empty objects sharing a
/// shape, and disqualifying otherwise.
fn tabular_fields_v4(values: &[Value]) -> Option<Vec<FieldNodeV4>> {
    let rows = values
        .iter()
        .map(|value| match value {
            Value::Object(document) => Some(document),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let first = rows.first()?;
    let first_keys = first
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect::<Vec<_>>();
    if first_keys.is_empty() {
        return None;
    }
    for row in &rows {
        if row.fields.len() != first_keys.len()
            || first_keys.iter().any(|key| row.get(key).is_none())
        {
            return None;
        }
    }

    let mut fields = Vec::with_capacity(first_keys.len());
    for name in first_keys {
        let column = rows
            .iter()
            .map(|row| row.get(name).expect("key set verified above"))
            .collect::<Vec<_>>();
        if column.iter().all(|value| value.is_primitive()) {
            fields.push(FieldNodeV4 {
                name: name.to_owned(),
                children: None,
            });
            continue;
        }
        if !column
            .iter()
            .all(|value| matches!(value, Value::Object(document) if !document.fields.is_empty()))
        {
            return None;
        }
        let child_values = column.into_iter().cloned().collect::<Vec<_>>();
        let children = tabular_fields_v4(&child_values)?;
        fields.push(FieldNodeV4 {
            name: name.to_owned(),
            children: Some(children),
        });
    }
    Some(fields)
}

/// Keyed form additionally requires at least two non-empty object entries.
fn keyed_fields_v4(document: &Document) -> Option<Vec<FieldNodeV4>> {
    if document.fields.len() < 2 {
        return None;
    }
    if !document
        .fields
        .iter()
        .all(|field| matches!(&field.value, Value::Object(nested) if !nested.fields.is_empty()))
    {
        return None;
    }
    let rows = document
        .fields
        .iter()
        .map(|field| field.value.clone())
        .collect::<Vec<_>>();
    tabular_fields_v4(&rows)
}

fn collect_leaves_v4(value: &Value, fields: &[FieldNodeV4]) -> Vec<Value> {
    let Value::Object(document) = value else {
        unreachable!("shape detection verified object rows");
    };
    let mut leaves = Vec::new();
    for field in fields {
        let child = document
            .get(&field.name)
            .expect("shape detection verified the field is present");
        match &field.children {
            None => leaves.push(child.clone()),
            Some(children) => leaves.extend(collect_leaves_v4(child, children)),
        }
    }
    leaves
}

// ---------------------------------------------------------------------------
// Lexical helpers
// ---------------------------------------------------------------------------

fn header(
    key: Option<&str>,
    length: usize,
    fields: Option<&[FieldNodeV4]>,
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

fn format_fields(fields: &[FieldNodeV4], delimiter: char) -> String {
    fields
        .iter()
        .map(|field| match &field.children {
            None => canonical_key(&field.name),
            Some(children) => {
                format!("{}{{{}}}", canonical_key(&field.name), format_fields(children, delimiter))
            }
        })
        .collect::<Vec<_>>()
        .join(&delimiter.to_string())
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
