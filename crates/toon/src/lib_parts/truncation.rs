// Truncation detection over the one decode event stream.

/// Reports incomplete TOON with explicit decode options, without weakening
/// fail-fast decode.
pub fn detect_truncation_with_options(
    input: &str,
    options: &DecodeOptions,
) -> TruncationReport {
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
