// Whole-value v4.1 decode bridge for the surviving Rust extensions.

/// Whole-document convenience over the event stream: decodes to a JSON value.
pub fn decode_value_v4(input: &str, options: &DecodeStreamOptions) -> Result<Value, ParseError> {
    let mut value = build_value_from_event_results(decode_event_stream(input, options))?;
    if options.cyclic_discriminated_arrays {
        if let Value::Object(document) = value {
            value = Value::Object(expand_cyclic_discriminated_arrays(document)?);
        }
    }
    Ok(value)
}

/// Reports incomplete v4.1 TOON without weakening fail-fast decode.
pub fn detect_truncation_v4(input: &str, options: &DecodeStreamOptions) -> TruncationReport {
    let error = match decode_value_v4(input, options) {
        Ok(_) => return TruncationReport::complete(),
        Err(error) => error,
    };
    let ctx = StreamCtx {
        indent_size: options.indent.max(1),
        strict: options.strict,
        object_array_columns: options.object_array_columns,
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
                    let unit = if header.inline.is_some() {
                        "items"
                    } else {
                        "rows"
                    };
                    return TruncationReport::truncated(
                        TruncationKind::ArrayLengthMismatch,
                        lines.last().map_or(line.number, |last| last.number),
                        Some(header.length),
                        Some(actual),
                        format!("declared {} {unit} but received {actual}", header.length),
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
