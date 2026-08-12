/// Decoder configuration for the authoritative TOON v4.1 codec.
pub type DecodeOptions = DecodeStreamOptions;

/// Positioned error returned by the authoritative TOON v4.1 decoder.
pub type DecodeError = ParseError;

/// Decodes a complete TOON v4.1 value from a string.
///
/// ```
/// use reddb_io_toon::decode;
///
/// let value = decode("answer: 42\n")?;
/// assert_eq!(value.to_json_value(), serde_json::json!({"answer": 42}));
/// # Ok::<(), reddb_io_toon::DecodeError>(())
/// ```
pub fn decode(input: &str) -> Result<Value, DecodeError> {
    decode_with_options(input, &DecodeOptions::default())
}

/// Decodes a complete TOON v4.1 value from a string with explicit options.
pub fn decode_with_options(
    input: &str,
    options: &DecodeOptions,
) -> Result<Value, DecodeError> {
    let mut value = build_value_from_event_results(decode_event_stream(input, options))?;
    if options.cyclic_discriminated_arrays {
        if let Value::Object(document) = value {
            value = Value::Object(expand_cyclic_discriminated_arrays(document)?);
        }
    }
    Ok(value)
}

/// Decodes a complete TOON v4.1 value from buffered input.
///
/// ```
/// use reddb_io_toon::decode_reader;
/// use std::io::Cursor;
///
/// let value = decode_reader(Cursor::new(b"ready: true\n"))?;
/// assert_eq!(value.to_json_value(), serde_json::json!({"ready": true}));
/// # Ok::<(), reddb_io_toon::DecodeError>(())
/// ```
pub fn decode_reader<R: BufRead>(reader: R) -> Result<Value, DecodeError> {
    decode_reader_with_options(reader, &DecodeOptions::default())
}

/// Decodes a complete TOON v4.1 value from buffered input with explicit options.
pub fn decode_reader_with_options<R: BufRead>(
    mut reader: R,
    options: &DecodeOptions,
) -> Result<Value, DecodeError> {
    let mut input = String::new();
    std::io::Read::read_to_string(&mut reader, &mut input).map_err(|_| ParseError {
        line: 1,
        message: "failed to read input",
        max_depth: None,
    })?;
    decode_with_options(&input, options)
}

/// Returns a lazy iterator over positioned TOON v4.1 events.
///
/// ```
/// use reddb_io_toon::decode_iter;
///
/// let events = decode_iter("answer: 42\n").collect::<Result<Vec<_>, _>>()?;
/// assert!(!events.is_empty());
/// # Ok::<(), reddb_io_toon::DecodeError>(())
/// ```
pub fn decode_iter(input: &str) -> EventDecoder {
    decode_iter_with_options(input, &DecodeOptions::default())
}

/// Returns a lazy iterator over positioned TOON v4.1 events with explicit options.
pub fn decode_iter_with_options(input: &str, options: &DecodeOptions) -> EventDecoder {
    decode_event_stream(input, options)
}

/// Encodes a value as canonical TOON v4.1.
///
/// ```
/// use reddb_io_toon::{encode, Value};
///
/// let value = Value::from_json_value(serde_json::json!({"answer": 42}));
/// assert_eq!(encode(&value)?, "answer: 42");
/// # Ok::<(), reddb_io_toon::EncodeError>(())
/// ```
pub fn encode(value: &Value) -> Result<String, EncodeError> {
    encode_with_options(value, EncodeV4Options::default())
}

/// Encodes a value as TOON v4.1 with explicit options.
pub fn encode_with_options(
    value: &Value,
    options: EncodeV4Options,
) -> Result<String, EncodeError> {
    encode_v4(value, options)
}

fn decode_options_from_legacy(options: ParseOptions) -> DecodeOptions {
    DecodeOptions {
        indent: options.indent,
        strict: options.strict,
        cyclic_discriminated_arrays: options.cyclic_discriminated_arrays,
        object_array_columns: true,
        max_depth: options.max_depth,
    }
}

fn encode_options_from_legacy(options: EncodeOptions) -> EncodeV4Options {
    EncodeV4Options {
        delimiter: options.delimiter,
        indent_size: DEFAULT_INDENT,
        primitive_array_columns: options.primitive_array_columns,
        object_array_columns: options.object_array_columns,
        cyclic_discriminated_arrays: options.cyclic_discriminated_arrays,
        max_depth: options.max_depth,
    }
}

impl ParseError {
    /// The stable decoder reason without the source-position prefix.
    pub fn reason(&self) -> &'static str {
        self.message
    }
}

/// Reports incomplete authoritative v4.1 TOON using default options.
pub fn detect_truncation(input: &str) -> TruncationReport {
    detect_truncation_v4(input, &DecodeOptions::default())
}

/// Reports incomplete authoritative v4.1 TOON with compatibility-shaped options.
pub fn detect_truncation_with_options(
    input: &str,
    options: ParseOptions,
) -> TruncationReport {
    detect_truncation_v4(input, &decode_options_from_legacy(options))
}
