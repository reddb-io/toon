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
    let (error, span) = decode_events_for_truncation(input, options);
    let Some(error) = error else {
        return TruncationReport::complete();
    };
    if let Some(span) = span {
        return TruncationReport::truncated(
            TruncationKind::ArrayLengthMismatch,
            span.line,
            Some(span.declared),
            Some(span.actual),
            format!(
                "declared {} {} but received {}",
                span.declared, span.unit, span.actual
            ),
        );
    }
    TruncationReport::truncated(
        TruncationKind::Invalid,
        error.line(),
        None,
        None,
        error.to_string(),
    )
}
