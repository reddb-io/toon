// ---------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------

/// A decoder-visible number: `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`.
/// Leading zeros in the integer part make the token a string (§4).
fn is_number_token(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;

    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }

    let integer_start = index;
    if !consume_digits(bytes, &mut index) {
        return false;
    }
    if index - integer_start > 1 && bytes[integer_start] == b'0' {
        return false;
    }

    if bytes.get(index) == Some(&b'.') {
        index += 1;
        if !consume_digits(bytes, &mut index) {
            return false;
        }
    }

    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        if !consume_digits(bytes, &mut index) {
            return false;
        }
    }

    index == bytes.len()
}

/// The §7.2 "numeric-like" test used for quoting: unlike [`is_number_token`] it
/// also matches leading-zero forms such as `05`, which decode as strings but
/// must still be quoted so they never decode as numbers.
fn is_numeric_like(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;

    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    if !consume_digits(bytes, &mut index) {
        return false;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        if !consume_digits(bytes, &mut index) {
            return false;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        if !consume_digits(bytes, &mut index) {
            return false;
        }
    }

    index == bytes.len()
}

fn consume_digits(bytes: &[u8], index: &mut usize) -> bool {
    let start = *index;
    while matches!(bytes.get(*index), Some(b'0'..=b'9')) {
        *index += 1;
    }
    *index > start
}

/// Canonical decimal form per §2: no exponent inside `[1e-6, 1e21)`, no trailing
/// fractional zeros, `-0` normalized to `0`.
fn canonical_number(value: &str) -> String {
    if !is_number_token(value) {
        return value.to_owned();
    }

    // Plain integers are kept verbatim so precision beyond f64 survives.
    if !value.contains(['.', 'e', 'E']) {
        if value
            .trim_start_matches('-')
            .chars()
            .all(|digit| digit == '0')
        {
            return "0".to_owned();
        }
        return value.to_owned();
    }

    let Ok(number) = value.parse::<f64>() else {
        return value.to_owned();
    };
    if number == 0.0 {
        return "0".to_owned();
    }
    if number.fract() == 0.0 && number.abs() < 1e21 {
        return format!("{}", number as i128);
    }
    format!("{number}")
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

fn check_encode_depth(depth: usize, options: EncodeOptions) -> Result<(), EncodeError> {
    validate_encode_delimiter(options.delimiter)?;
    if options.max_depth != 0 && depth > options.max_depth {
        return Err(EncodeError {
            message: "maximum nesting depth exceeded",
            max_depth: Some(options.max_depth),
        });
    }
    Ok(())
}

fn validate_encode_delimiter(delimiter: char) -> Result<(), EncodeError> {
    if matches!(delimiter, DOCUMENT_DELIMITER | '|' | '\t') {
        return Ok(());
    }
    Err(EncodeError {
        message: "invalid array header",
        max_depth: None,
    })
}

fn write_indent(output: &mut String, depth: usize) {
    for _ in 0..depth * DEFAULT_INDENT {
        output.push(' ');
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CyclicEncodedSection {
    key: String,
    shape: CyclicArrayShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CyclicArrayShape {
    discriminator: String,
    len: usize,
    common: Vec<String>,
    common_rows: Vec<Document>,
    order: CyclicOrder,
    groups: Vec<CyclicGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CyclicOrder {
    cycle: Vec<String>,
    repeats: usize,
    encoded: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CyclicGroup {
    label: String,
    fields: Vec<String>,
    rows: Vec<Document>,
}

fn write_cyclic_discriminated_arrays(
    output: &mut String,
    document: &Document,
    options: EncodeOptions,
) -> Result<bool, EncodeError> {
    if !options.cyclic_discriminated_arrays || document.fields.is_empty() {
        return Ok(false);
    }
    check_encode_depth(0, options)?;

    let mut seen_keys = Vec::new();
    let mut sections = Vec::with_capacity(document.fields.len());
    for field in &document.fields {
        if seen_keys.contains(&field.key) || !is_cyclic_header_token(&field.key) {
            return Ok(false);
        }
        seen_keys.push(field.key.clone());
        let Value::Array(array) = &field.value else {
            return Ok(false);
        };
        let values = array.values();
        let Some(shape) = cyclic_array_shape(&values, 1, options)? else {
            return Ok(false);
        };
        sections.push(CyclicEncodedSection {
            key: field.key.clone(),
            shape,
        });
    }

    for section in &sections {
        write_cyclic_section(output, section);
    }
    Ok(true)
}

fn cyclic_array_shape(
    values: &[Value],
    depth: usize,
    options: EncodeOptions,
) -> Result<Option<CyclicArrayShape>, EncodeError> {
    check_encode_depth(depth, options)?;
    let rows = values
        .iter()
        .map(|value| match value {
            Value::Object(document) if document_has_unique_keys(document) => Some(document),
            _ => None,
        })
        .collect::<Option<Vec<_>>>();
    let Some(rows) = rows else {
        return Ok(None);
    };

    let Some(discriminator) = cyclic_discriminator(&rows) else {
        return Ok(None);
    };
    if !is_cyclic_header_token(&discriminator) {
        return Ok(None);
    }
    let labels = rows
        .iter()
        .map(|row| match row.get(&discriminator) {
            Some(Value::String(label)) => Some(label.clone()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .expect("cyclic_discriminator checked string labels");
    let Some(order) = cyclic_order(&labels) else {
        return Ok(None);
    };

    let common = cyclic_common_fields(&rows, &discriminator);
    if common.iter().any(|key| !is_cyclic_header_token(key)) {
        return Ok(None);
    }
    let mut common_rows = Vec::with_capacity(rows.len());
    let mut groups_by_label: HashMap<String, Vec<Document>> = HashMap::new();
    for (row, label) in rows.iter().zip(&labels) {
        common_rows.push(Document {
            fields: common
                .iter()
                .map(|key| Field {
                    key: key.clone(),
                    value: row.get(key).expect("common fields are present").clone(),
                })
                .collect(),
        });
        let common_or_discriminator =
            |key: &str| key == discriminator || common_contains(&common, key);
        let mut payload = Document::default();
        for field in row
            .fields
            .iter()
            .filter(|field| !common_or_discriminator(&field.key))
        {
            if !flatten_cyclic_value(&field.value, &field.key, &mut payload) {
                return Ok(None);
            }
        }
        groups_by_label
            .entry(label.clone())
            .or_default()
            .push(payload);
    }
    let mut groups = Vec::with_capacity(order.cycle.len());
    for label in &order.cycle {
        let rows = groups_by_label
            .remove(label)
            .expect("cycle labels come from the row labels");
        let Some(fields) = cyclic_uniform_fields(&rows) else {
            return Ok(None);
        };
        if fields.is_empty() {
            return Ok(None);
        }
        groups.push(CyclicGroup {
            label: label.clone(),
            fields,
            rows,
        });
    }

    Ok(Some(CyclicArrayShape {
        discriminator,
        len: values.len(),
        common,
        common_rows,
        order,
        groups,
    }))
}

fn document_has_unique_keys(document: &Document) -> bool {
    let mut keys = Vec::with_capacity(document.fields.len());
    for field in &document.fields {
        if keys.contains(&field.key) {
            return false;
        }
        keys.push(field.key.clone());
    }
    true
}

fn cyclic_discriminator(rows: &[&Document]) -> Option<String> {
    for key in ["type", "kind", "event"] {
        if rows
            .iter()
            .all(|row| matches!(row.get(key), Some(Value::String(_))))
        {
            return Some(key.to_owned());
        }
    }
    None
}

fn cyclic_order(labels: &[String]) -> Option<CyclicOrder> {
    for size in 2..=8.min(labels.len() / 3) {
        let cycle = labels[..size].to_vec();
        if !all_unique(&cycle) {
            continue;
        }
        let repeats = labels.len() / size;
        if repeats < 3 || repeats * size != labels.len() {
            continue;
        }
        if labels
            .iter()
            .enumerate()
            .any(|(index, label)| label != &cycle[index % size])
        {
            continue;
        }
        let encoded = format!(
            "cycle({})*{repeats}",
            cycle
                .iter()
                .map(|label| percent_encode(label))
                .collect::<Vec<_>>()
                .join(",")
        );
        let raw = labels
            .iter()
            .map(|label| percent_encode(label))
            .collect::<Vec<_>>()
            .join(",");
        if encoded.len() * 10 <= raw.len() * 4 {
            return Some(CyclicOrder {
                cycle,
                repeats,
                encoded,
            });
        }
    }
    None
}

fn all_unique(values: &[String]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(index, value)| !values[..index].contains(value))
}

fn cyclic_common_fields(rows: &[&Document], discriminator: &str) -> Vec<String> {
    let Some(first) = rows.first() else {
        return Vec::new();
    };
    let Some(discriminator_index) = first
        .fields
        .iter()
        .position(|field| field.key == discriminator)
    else {
        return Vec::new();
    };

    let mut common = Vec::new();
    for field in first.fields.iter().skip(discriminator_index + 1) {
        if rows
            .iter()
            .all(|row| row.get(&field.key).is_some_and(Value::is_primitive))
        {
            common.push(field.key.clone());
        } else {
            break;
        }
    }
    common
}

fn common_contains(common: &[String], key: &str) -> bool {
    common.iter().any(|common_key| common_key == key)
}

fn is_cyclic_header_token(value: &str) -> bool {
    !value.is_empty() && is_bare_cyclic_path(value)
}

fn flatten_cyclic_value(value: &Value, prefix: &str, output: &mut Document) -> bool {
    if !is_bare_cyclic_path(prefix) {
        return false;
    }
    if value.is_primitive() {
        output.fields.push(Field {
            key: prefix.to_owned(),
            value: value.clone(),
        });
        return true;
    }
    match value {
        Value::Array(array) => {
            let values = array.values();
            output.fields.push(Field {
                key: format!("{prefix}.length"),
                value: Value::Number(values.len().to_string()),
            });
            values.iter().enumerate().all(|(index, value)| {
                flatten_cyclic_value(value, &format!("{prefix}.{index}"), output)
            })
        }
        Value::Object(document) => document.fields.iter().all(|field| {
            !field.key.contains('.')
                && flatten_cyclic_value(&field.value, &format!("{prefix}.{}", field.key), output)
        }),
        _ => false,
    }
}

fn cyclic_uniform_fields(rows: &[Document]) -> Option<Vec<String>> {
    let fields: Vec<String> = rows
        .first()
        .map(|row| row.fields.iter().map(|field| field.key.clone()).collect())?;
    if rows.iter().all(|row| {
        row.fields.len() == fields.len()
            && row
                .fields
                .iter()
                .zip(&fields)
                .all(|(field, expected)| field.key == *expected)
    }) && fields.iter().all(|field| is_bare_cyclic_path(field))
    {
        Some(fields)
    } else {
        None
    }
}

fn inflate_cyclic_document(document: &Document, line: usize) -> Result<Value, ParseError> {
    if let Some(length) = document.get("length").and_then(value_to_usize) {
        let mut values = Vec::with_capacity(length);
        for index in 0..length {
            let key = index.to_string();
            let Some(value) = document.get(&key) else {
                return Err(cyclic_invalid(line));
            };
            values.push(inflate_cyclic_value(value, line)?);
        }
        return Ok(Value::Array(Array::List(values)));
    }
    let mut inflated = Document::default();
    for field in &document.fields {
        inflated.fields.push(Field {
            key: field.key.clone(),
            value: inflate_cyclic_value(&field.value, line)?,
        });
    }
    Ok(Value::Object(inflated))
}

fn inflate_cyclic_flat_document(document: &Document, line: usize) -> Result<Value, ParseError> {
    let mut nested = Document::default();
    for field in &document.fields {
        let path = field.key.split('.').map(str::to_owned).collect::<Vec<_>>();
        insert_tabular_path(&mut nested, &path, field.value.clone());
    }
    inflate_cyclic_document(&nested, line)
}

fn inflate_cyclic_value(value: &Value, line: usize) -> Result<Value, ParseError> {
    match value {
        Value::Object(document) => inflate_cyclic_document(document, line),
        Value::Array(array) => Ok(Value::Array(Array::List(
            array
                .values()
                .into_iter()
                .map(|value| inflate_cyclic_value(&value, line))
                .collect::<Result<Vec<_>, _>>()?,
        ))),
        value => Ok(value.clone()),
    }
}

fn is_bare_cyclic_path(value: &str) -> bool {
    value.split('.').all(|segment| {
        !segment.is_empty()
            && (segment.bytes().all(|byte| byte.is_ascii_digit()) || {
                let mut bytes = segment.bytes();
                bytes
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                    && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
    })
}

fn write_cyclic_section(output: &mut String, section: &CyclicEncodedSection) {
    output.push_str(&canonical_key(&section.key));
    output.push_str(":\n");
    output.push_str("  order: ");
    output.push_str(&primitive_text(
        &Value::String(section.shape.order.encoded.clone()),
        CYCLIC_TABLE_DELIMITER,
    ));
    output.push('\n');
    output.push_str("  discriminator: ");
    output.push_str(&primitive_text(
        &Value::String(section.shape.discriminator.clone()),
        CYCLIC_TABLE_DELIMITER,
    ));
    output.push('\n');
    output.push_str("  rows: ");
    output.push_str(&section.shape.len.to_string());
    output.push('\n');
    if !section.shape.common.is_empty() {
        write_cyclic_table(
            output,
            "common",
            &section.shape.common,
            &section.shape.common_rows,
        );
    }
    for group in &section.shape.groups {
        write_cyclic_table(output, &group.label, &group.fields, &group.rows);
    }
}

fn write_cyclic_table(output: &mut String, key: &str, fields: &[String], rows: &[Document]) {
    output.push_str("  ");
    output.push_str(&canonical_key(key));
    output.push('[');
    output.push_str(&rows.len().to_string());
    output.push(CYCLIC_TABLE_DELIMITER);
    output.push_str("]{");
    output.push_str(
        &fields
            .iter()
            .map(|field| canonical_key(field))
            .collect::<Vec<_>>()
            .join(&CYCLIC_TABLE_DELIMITER.to_string()),
    );
    output.push_str("}:\n");
    for row in rows {
        output.push_str("    ");
        let cells = fields
            .iter()
            .map(|field| {
                primitive_text(
                    row.get(field)
                        .expect("cyclic table shape checked row fields"),
                    CYCLIC_TABLE_DELIMITER,
                )
            })
            .collect::<Vec<_>>();
        output.push_str(&cells.join(&CYCLIC_TABLE_DELIMITER.to_string()));
        output.push('\n');
    }
}

fn percent_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(char::from(*byte));
            }
            byte => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

/// Writes `key: value` at the caller's cursor (indent or `- ` already emitted).
fn write_field(
    output: &mut String,
    key: &str,
    value: &Value,
    depth: usize,
    options: EncodeOptions,
) -> Result<(), EncodeError> {
    check_encode_depth(depth, options)?;
    match value {
        Value::Array(array) => {
            write_array(output, Some(key), &array.values(), depth, false, options)?;
        }
        Value::Object(document) => {
            if let Some(shape) = keyed_map_shape(document, options, depth + 1)? {
                write_keyed_map(output, key, document, &shape, depth, options)?;
                return Ok(());
            }
            output.push_str(&canonical_key(key));
            output.push_str(":\n");
            document.write_fields(output, depth + 1, options)?;
        }
        value => {
            output.push_str(&canonical_key(key));
            output.push_str(": ");
            output.push_str(&primitive_text(value, options.delimiter));
            output.push('\n');
        }
    }
    Ok(())
}

/// Writes an array header plus body. `list_item` selects the empty-array form:
/// `[0]:` inside a list (§9.2) versus `key: []` / `[]` elsewhere (§9.1).
fn write_array(
    output: &mut String,
    key: Option<&str>,
    values: &[Value],
    depth: usize,
    list_item: bool,
    options: EncodeOptions,
) -> Result<(), EncodeError> {
    check_encode_depth(depth, options)?;
    if values.is_empty() {
        match key {
            Some(key) => {
                output.push_str(&canonical_key(key));
                output.push_str(": []\n");
            }
            None if list_item => output.push_str("[0]:\n"),
            None => output.push_str("[]\n"),
        }
        return Ok(());
    }

    if values.iter().all(Value::is_primitive) {
        write_array_header(output, key, values.len(), None, options.delimiter);
        output.push(' ');
        let cells = values
            .iter()
            .map(|value| primitive_text(value, options.delimiter))
            .collect::<Vec<_>>();
        output.push_str(&cells.join(&options.delimiter.to_string()));
        output.push('\n');
        return Ok(());
    }

    // In list-item position the tabular form has nowhere to put its field list,
    // so §9.4 requires the expanded list even for a uniform array of objects.
    if let Some(shape) = if list_item {
        None
    } else {
        tabular_shape(values, options, depth + 1)?
    } {
        write_array_header(
            output,
            key,
            values.len(),
            Some(&shape.fields),
            options.delimiter,
        );
        output.push('\n');
        for value in values {
            write_indent(output, depth + 1);
            let mut child_output = String::new();
            let cells = shape
                .paths
                .iter()
                .map(|path| {
                    let cell =
                        value_at_path(value, &path.path).expect("tabular_shape checked the path");
                    column_text(
                        cell,
                        path,
                        options.delimiter,
                        options,
                        &mut child_output,
                        depth + 2,
                    )
                })
                .collect::<Vec<_>>();
            output.push_str(&cells.join(&options.delimiter.to_string()));
            output.push('\n');
            output.push_str(&child_output);
        }
        return Ok(());
    }

    write_array_header(output, key, values.len(), None, options.delimiter);
    output.push('\n');
    for value in values {
        write_indent(output, depth + 1);
        write_list_item(output, value, depth + 1, options)?;
    }
    Ok(())
}

fn write_list_item(
    output: &mut String,
    value: &Value,
    depth: usize,
    options: EncodeOptions,
) -> Result<(), EncodeError> {
    check_encode_depth(depth, options)?;
    match value {
        Value::Object(document) if document.fields.is_empty() => output.push_str("-\n"),
        Value::Object(document) => {
            output.push_str("- ");
            let first = &document.fields[0];
            write_field(output, &first.key, &first.value, depth + 1, options)?;
            for field in &document.fields[1..] {
                write_indent(output, depth + 1);
                write_field(output, &field.key, &field.value, depth + 1, options)?;
            }
        }
        Value::Array(array) => {
            output.push_str("- ");
            write_array(output, None, &array.values(), depth, true, options)?;
        }
        value => {
            output.push_str("- ");
            output.push_str(&primitive_text(value, options.delimiter));
            output.push('\n');
        }
    }
    Ok(())
}

fn write_array_header(
    output: &mut String,
    key: Option<&str>,
    len: usize,
    fields: Option<&[HeaderFieldShape]>,
    delimiter: char,
) {
    if let Some(key) = key {
        output.push_str(&canonical_key(key));
    }
    output.push('[');
    output.push_str(&len.to_string());
    push_delimiter_prefix(output, delimiter);
    output.push(']');
    if let Some(fields) = fields {
        output.push('{');
        let names = fields
            .iter()
            .map(|field| header_field_text(field, delimiter))
            .collect::<Vec<_>>();
        output.push_str(&names.join(&delimiter.to_string()));
        output.push('}');
    }
    output.push(':');
}

fn push_delimiter_prefix(output: &mut String, delimiter: char) {
    if delimiter != DOCUMENT_DELIMITER {
        output.push(delimiter);
    }
}

fn delimiter_prefix_text(delimiter: char) -> String {
    if delimiter == DOCUMENT_DELIMITER {
        String::new()
    } else {
        delimiter.to_string()
    }
}

fn write_keyed_map(
    output: &mut String,
    key: &str,
    document: &Document,
    shape: &TabularShape,
    depth: usize,
    options: EncodeOptions,
) -> Result<(), EncodeError> {
    check_encode_depth(depth, options)?;
    output.push_str(&canonical_key(key));
    output.push('{');
    push_delimiter_prefix(output, options.delimiter);
    let names = shape
        .fields
        .iter()
        .map(|field| header_field_text(field, options.delimiter))
        .collect::<Vec<_>>();
    output.push_str(&names.join(&options.delimiter.to_string()));
    output.push_str("}:\n");
    for field in &document.fields {
        let Value::Object(row) = &field.value else {
            unreachable!("keyed_map_shape checked row values");
        };
        write_indent(output, depth + 1);
        output.push_str(&canonical_key(&field.key));
        output.push_str(": ");
        let row_value = Value::Object(row.clone());
        let mut child_output = String::new();
        let cells = shape
            .paths
            .iter()
            .map(|path| {
                let cell = value_at_path(&row_value, &path.path)
                    .expect("keyed_map_shape checked row paths");
                column_text(
                    cell,
                    path,
                    options.delimiter,
                    options,
                    &mut child_output,
                    depth + 2,
                )
            })
            .collect::<Vec<_>>();
        output.push_str(&cells.join(&options.delimiter.to_string()));
        output.push('\n');
        output.push_str(&child_output);
    }
    Ok(())
}

fn header_field_text(field: &HeaderFieldShape, delimiter: char) -> String {
    if let Some(list_delimiter) = field.list_delimiter {
        return format!("{}[{list_delimiter}]", canonical_key(&field.key));
    }
    if let Some(fixed_len) = field.fixed_len {
        return format!(
            "{}[{fixed_len}{}]",
            canonical_key(&field.key),
            delimiter_prefix_text(delimiter)
        );
    }
    if field.children.is_empty() {
        return canonical_key(&field.key);
    }
    let children = field
        .children
        .iter()
        .map(|child| header_field_text(child, delimiter))
        .collect::<Vec<_>>()
        .join(&delimiter.to_string());
    format!("{}{{{children}}}", canonical_key(&field.key))
}

