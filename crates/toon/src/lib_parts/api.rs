/// Decoder configuration for the authoritative TOON v4.1 codec.
pub type DecodeOptions = DecodeStreamOptions;

/// Positioned error returned by the authoritative TOON v4.1 decoder.
pub type DecodeError = ParseError;

/// Compatibility options for the pre-v4.1 parser.
pub type LegacyParseOptions = ParseOptions;

/// Compatibility options for the pre-v4.1 serializer.
pub type LegacyEncodeOptions = EncodeOptions;

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

impl Document {
    /// Parses an object root with the pre-v4.1 compatibility parser.
    pub fn parse_legacy(input: &str) -> Result<Self, ParseError> {
        Self::parse_legacy_with_options(input, LegacyParseOptions::default())
    }

    /// Parses an object root with explicit pre-v4.1 compatibility options.
    pub fn parse_legacy_with_options(
        input: &str,
        options: LegacyParseOptions,
    ) -> Result<Self, ParseError> {
        match Value::parse_legacy_with_options(input, options)? {
            Value::Object(document) => Ok(document),
            _ => Err(ParseError {
                line: 1,
                message: "expected `key: value`",
                max_depth: None,
            }),
        }
    }

    pub fn to_legacy_toon(&self) -> String {
        self.to_legacy_toon_with_options(LegacyEncodeOptions::default())
    }

    pub fn to_legacy_toon_with_options(&self, options: LegacyEncodeOptions) -> String {
        self.try_to_legacy_toon_with_options(options)
            .expect("legacy TOON encoding failed")
    }

    pub fn try_to_legacy_toon(&self) -> Result<String, EncodeError> {
        self.try_to_legacy_toon_with_options(LegacyEncodeOptions::default())
    }

    pub fn try_to_legacy_toon_with_options(
        &self,
        options: LegacyEncodeOptions,
    ) -> Result<String, EncodeError> {
        let mut output = String::new();
        if write_cyclic_discriminated_arrays(&mut output, self, options)? {
            return Ok(output);
        }
        self.write_fields(&mut output, 0, options)?;
        Ok(output)
    }
}

impl Value {
    /// Parses a value with the pre-v4.1 compatibility parser.
    pub fn parse_legacy(input: &str) -> Result<Self, ParseError> {
        Self::parse_legacy_with_options(input, LegacyParseOptions::default())
    }

    /// Parses a value with explicit pre-v4.1 compatibility options.
    pub fn parse_legacy_with_options(
        input: &str,
        options: LegacyParseOptions,
    ) -> Result<Self, ParseError> {
        let options = ParseOptions {
            indent: options.indent.max(1),
            ..options
        };
        let lines = collect_lines(input, &options)?;
        let Some(first) = lines.first() else {
            return Ok(Self::Object(Document::default()));
        };
        if first.depth != 0 {
            return Err(ParseError {
                line: first.number,
                message: "invalid indentation",
                max_depth: None,
            });
        }

        let only_line = lines.len() == 1;
        if only_line && first.content.trim() == "[]" {
            return Ok(Self::Array(Array::List(Vec::new())));
        }
        if first.content.starts_with('[') {
            check_header_depth(first.content, first.number, &options)?;
            match parse_header(
                first.content,
                find_unquoted(first.content, ':', first.number)?,
            ) {
                Ok(header) => return parse_root_array(header, &lines, &options),
                Err(error) if options.strict => return Err(error.at(first.number)),
                Err(_) => {}
            }
        }
        if only_line && find_unquoted(first.content, ':', first.number)?.is_none() {
            return parse_scalar(first.content.trim(), first.number);
        }

        let mut index = 0;
        let document = parse_object(&lines, &mut index, 0, &options)?;
        if let Some(line) = lines.get(index) {
            return Err(ParseError {
                line: line.number,
                message: "expected end of document",
                max_depth: None,
            });
        }
        let document = if options.cyclic_discriminated_arrays {
            expand_cyclic_discriminated_arrays(document)?
        } else {
            document
        };
        Ok(Self::Object(document))
    }

    pub fn to_legacy_toon(&self) -> String {
        self.to_legacy_toon_with_options(LegacyEncodeOptions::default())
    }

    pub fn to_legacy_toon_with_options(&self, options: LegacyEncodeOptions) -> String {
        self.try_to_legacy_toon_with_options(options)
            .expect("legacy TOON encoding failed")
    }

    pub fn try_to_legacy_toon(&self) -> Result<String, EncodeError> {
        self.try_to_legacy_toon_with_options(LegacyEncodeOptions::default())
    }

    pub fn try_to_legacy_toon_with_options(
        &self,
        options: LegacyEncodeOptions,
    ) -> Result<String, EncodeError> {
        let mut output = String::new();
        match self {
            Self::Object(document) => {
                if write_cyclic_discriminated_arrays(&mut output, document, options)? {
                    return Ok(output);
                }
                document.write_fields(&mut output, 0, options)?;
            }
            Self::Array(array) => {
                write_array(&mut output, None, &array.values(), 0, false, options)?
            }
            value => {
                validate_encode_delimiter(options.delimiter)?;
                output.push_str(&primitive_text(value, options.delimiter));
            }
        }
        Ok(output)
    }
}

impl Array {
    pub fn to_legacy_toon(&self) -> String {
        self.to_legacy_toon_with_options(LegacyEncodeOptions::default())
    }

    pub fn to_legacy_toon_with_options(&self, options: LegacyEncodeOptions) -> String {
        self.try_to_legacy_toon_with_options(options)
            .expect("legacy TOON encoding failed")
    }

    pub fn try_to_legacy_toon(&self) -> Result<String, EncodeError> {
        self.try_to_legacy_toon_with_options(LegacyEncodeOptions::default())
    }

    pub fn try_to_legacy_toon_with_options(
        &self,
        options: LegacyEncodeOptions,
    ) -> Result<String, EncodeError> {
        let mut output = String::new();
        write_array(&mut output, None, &self.values(), 0, false, options)?;
        Ok(output)
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
