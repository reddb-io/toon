// Whole-value v4.1 decode bridge for the surviving Rust extensions.

/// Whole-document convenience over the event stream: decodes to a JSON value.
pub fn decode_value_v4(input: &str, options: &DecodeStreamOptions) -> Result<Value, ParseError> {
    let mut value = if options.object_array_columns && has_fixed_array_column_header(input) {
        decode_extension_value(input, options)?
    } else {
        let (events, error) = decode_events(input, options);
        match error {
            None => build_value_from_events(&events),
            Some(_) if options.object_array_columns && has_child_table_header(input) => {
                decode_extension_value(input, options)?
            }
            Some(error) => return Err(error),
        }
    };
    if options.cyclic_discriminated_arrays {
        if let Value::Object(document) = value {
            value = Value::Object(expand_cyclic_discriminated_arrays(document)?);
        }
    }
    Ok(value)
}

fn decode_extension_value(
    input: &str,
    options: &DecodeStreamOptions,
) -> Result<Value, ParseError> {
    Value::parse_with_options(
        input,
        ParseOptions {
            indent: options.indent,
            strict: options.strict,
            cyclic_discriminated_arrays: false,
            max_depth: options.max_depth,
            ..ParseOptions::default()
        },
    )
}

fn has_fixed_array_column_header(input: &str) -> bool {
    input.lines().any(|line| {
        let Some(outer_close) = line.find(']') else {
            return false;
        };
        let Some(fields_start) = line[outer_close + 1..].find('{').map(|at| at + outer_close + 1)
        else {
            return false;
        };
        let Some(fields_end) = line[fields_start + 1..]
            .find("}:")
            .map(|at| at + fields_start + 1)
        else {
            return false;
        };
        contains_array_column_marker(&line[fields_start + 1..fields_end])
    })
}

fn has_child_table_header(input: &str) -> bool {
    input.lines().any(|line| {
        let Some(outer_close) = line.find(']') else {
            return false;
        };
        let Some(fields_start) = line[outer_close + 1..].find('{').map(|at| at + outer_close + 1)
        else {
            return false;
        };
        let Some(fields_end) = line.rfind('}') else {
            return false;
        };
        if fields_end <= fields_start {
            return false;
        }
        let fields = &line[fields_start + 1..fields_end];
        fields.contains('{') || contains_array_column_marker(fields)
    })
}

fn contains_array_column_marker(fields: &str) -> bool {
    let bytes = fields.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'[' {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        if cursor == bytes.len() {
            index += 1;
            continue;
        }
        if !bytes[cursor].is_ascii_digit() {
            cursor += 1;
            if cursor < bytes.len() && bytes[cursor] == b']' {
                return true;
            }
            index += 1;
            continue;
        }
        if bytes[cursor] == b'0' {
            cursor += 1;
        } else {
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
        }
        if cursor < bytes.len() && matches!(bytes[cursor], b'|' | b'\t') {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b']' {
            return true;
        }
        index += 1;
    }
    false
}

/// Reports incomplete v4.1 TOON without weakening fail-fast decode.
pub fn detect_truncation_v4(
    input: &str,
    options: &DecodeStreamOptions,
) -> TruncationReport {
    let error = match decode_value_v4(input, options) {
        Ok(_) => return TruncationReport::complete(),
        Err(error) => error,
    };
    let ctx = StreamCtx {
        indent_size: options.indent.max(1),
        strict: options.strict,
        max_depth: options.max_depth,
    };
    if error.message() == "array count mismatch" {
        if let Ok(lines) = classify_stream_lines(input, &ctx) {
            for (index, line) in lines.iter().enumerate() {
                let Ok(Some(header)) = parse_stream_header(&line.content, line.number) else {
                    continue;
                };
                let actual = if let Some(inline) = &header.inline {
                    split_stream_cells(inline, header.delimiter, line.number).len()
                } else {
                    lines
                        .iter()
                        .skip(index + 1)
                        .take_while(|nested| nested.depth > line.depth)
                        .filter(|nested| nested.depth == line.depth + 1)
                        .count()
                };
                if actual < header.length {
                    let unit = if header.inline.is_some() { "items" } else { "rows" };
                    return TruncationReport::truncated(
                        TruncationKind::ArrayLengthMismatch,
                        lines.last().map_or(line.number, |last| last.number),
                        Some(header.length),
                        Some(actual),
                        format!(
                            "declared {} {unit} but received {actual}",
                            header.length
                        ),
                    );
                }
            }
        }
    }
    TruncationReport::truncated(
        TruncationKind::Invalid,
        error.line(),
        None,
        None,
        error.to_string(),
    )
}
