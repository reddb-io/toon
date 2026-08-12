// ---------------------------------------------------------------------------
// TOONL headers, scalars, and shared lexical helpers
// ---------------------------------------------------------------------------






fn parse_fixed_width_list(value: &str) -> Option<(usize, char)> {
    let digits = value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
        return None;
    }
    let delimiter = match &value[digits.len()..] {
        "" => DOCUMENT_DELIMITER,
        "\t" => '\t',
        "|" => '|',
        _ => return None,
    };
    Some((digits.parse().ok()?, delimiter))
}


fn valid_list_delimiter(value: &str, active_delimiter: char) -> Option<char> {
    let mut characters = value.chars();
    let delimiter = characters.next()?;
    if characters.next().is_some()
        || delimiter == active_delimiter
        || matches!(
            delimiter,
            ' ' | '\t' | '\r' | '\n' | '"' | '[' | ']' | '{' | '}' | ':'
        )
    {
        return None;
    }
    Some(delimiter)
}

fn parse_toonl_header(
    line: &str,
    line_number: usize,
) -> Result<Option<ToonlHeaderLine>, ToonlError> {
    let Some(rest) = line.strip_prefix('[') else {
        return Ok(None);
    };
    let close_bracket = rest
        .find(']')
        .ok_or_else(|| toonl_error(line_number, "invalid header"))?;
    let bracket = &rest[..close_bracket];
    let (continuation, delimiter_text) = if let Some(delimiter_text) = bracket.strip_prefix('~') {
        (true, delimiter_text)
    } else {
        (false, bracket)
    };
    let delimiter = match delimiter_text {
        "" => DOCUMENT_DELIMITER,
        "|" => '|',
        "\t" => '\t',
        other if !continuation && other.starts_with('=') => return Ok(None),
        _ => return Err(toonl_error(line_number, "invalid header delimiter")),
    };
    let mut suffix = &rest[close_bracket + 1..];
    let mut tag = None;
    if !continuation && suffix.starts_with('<') {
        let tag_end = suffix
            .find('>')
            .ok_or_else(|| toonl_error(line_number, "invalid tag"))?;
        let tag_text = &suffix[1..tag_end];
        validate_toonl_tag(tag_text, line_number)?;
        if !delimiter_text.is_empty() {
            return Err(toonl_error(line_number, "invalid header delimiter"));
        }
        tag = Some(tag_text.to_owned());
        suffix = &suffix[tag_end + 1..];
    }
    if !suffix.starts_with('{') || !suffix.ends_with("}:") {
        return Err(toonl_error(line_number, "invalid header"));
    }

    let header_fields = suffix[1..suffix.len() - 2].to_owned();
    let fields = split_delimited(&header_fields, delimiter, line_number)
        .map_err(ToonlError::from_parse_error)?
        .into_iter()
        .map(|field| {
            let (field, _) =
                parse_key(&field, line_number).map_err(ToonlError::from_parse_error)?;
            if field.is_empty() {
                return Err(toonl_error(line_number, "invalid header fields"));
            }
            Ok(field)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if fields.is_empty() {
        return Err(toonl_error(line_number, "invalid header fields"));
    }

    Ok(Some(ToonlHeaderLine {
        delimiter,
        fields,
        header_fields,
        continuation,
        tag,
    }))
}

fn validate_toonl_tag(tag: &str, line_number: usize) -> Result<(), ToonlError> {
    if tag.is_empty()
        || !tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(toonl_error(line_number, "invalid tag"));
    }
    Ok(())
}

fn toonl_tagged_row_prefix(
    line: &str,
    line_number: usize,
) -> Result<Option<(&str, &str)>, ToonlError> {
    let Some(colon) = line.find(':') else {
        return Ok(None);
    };
    if colon == 0 {
        return Ok(None);
    }
    let tag = &line[..colon];
    if validate_toonl_tag(tag, line_number).is_ok() {
        return Ok(Some((tag, &line[colon + 1..])));
    }
    if tag
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        && tag.bytes().all(|byte| {
            !matches!(
                byte,
                b',' | b'[' | b']' | b'|' | b'{' | b'}' | b':' | b'\t' | b' '
            )
        })
    {
        return Err(toonl_error(line_number, "invalid tag"));
    }
    Ok(None)
}

fn ensure_continuation_matches(
    current: Option<&ToonlSegment>,
    header: &ToonlHeaderLine,
    line_number: usize,
) -> Result<(), ToonlError> {
    let Some(segment) = current else {
        return Err(toonl_error(
            line_number,
            "continuation header before header",
        ));
    };
    if segment.delimiter != header.delimiter || segment.header_fields != header.header_fields {
        return Err(toonl_error(line_number, "continuation header mismatch"));
    }
    Ok(())
}

fn ensure_open_continuation_matches(
    current: Option<&OpenToonlSegment>,
    header: &ToonlHeaderLine,
    line_number: usize,
) -> Result<(), ToonlError> {
    let Some(segment) = current else {
        return Err(toonl_error(
            line_number,
            "continuation header before header",
        ));
    };
    if segment.delimiter != header.delimiter || segment.header_fields != header.header_fields {
        return Err(toonl_error(line_number, "continuation header mismatch"));
    }
    Ok(())
}

fn toonl_header_text(delimiter: char, header_fields: &str, continuation: bool) -> String {
    let mut output = String::new();
    output.push('[');
    if continuation {
        output.push('~');
    }
    if delimiter != DOCUMENT_DELIMITER {
        output.push(delimiter);
    }
    output.push_str("]{");
    output.push_str(header_fields);
    output.push_str("}:\n");
    output
}

fn tagged_toonl_header_text(tag: &str, header_fields: &str) -> String {
    format!("[]<{tag}>{{{header_fields}}}:\n")
}

fn normalize_toonl_header_fields<T: AsRef<str>>(fields: &[T]) -> Result<Vec<String>, ToonlError> {
    if fields.is_empty() {
        return Err(toonl_error(0, "TOONL header requires fields"));
    }
    fields
        .iter()
        .map(|field| {
            let (field, _) = parse_key(field.as_ref(), 0).map_err(ToonlError::from_parse_error)?;
            if field.is_empty() {
                return Err(toonl_error(0, "TOONL header requires fields"));
            }
            Ok(field)
        })
        .collect()
}

fn toonl_header_fields(delimiter: char, fields: &[String]) -> String {
    fields
        .iter()
        .map(|field| canonical_key(field))
        .collect::<Vec<_>>()
        .join(&delimiter.to_string())
}

fn validate_continuation_cadence(cadence: Option<usize>) -> Result<(), ToonlError> {
    if cadence == Some(0) {
        return Err(toonl_error(
            0,
            "TOONL continuation cadence must be positive",
        ));
    }
    Ok(())
}

fn continuation_due(
    every_rows: Option<usize>,
    rows_since: usize,
    every_bytes: Option<usize>,
    bytes_since: usize,
) -> bool {
    every_rows.is_some_and(|rows| rows_since >= rows)
        || every_bytes.is_some_and(|bytes| bytes_since >= bytes)
}

fn toonl_trailer_count(line: &str, line_number: usize) -> Result<Option<usize>, ToonlError> {
    if !(line.starts_with("[=") && line.ends_with(']')) {
        return Ok(None);
    }
    line[2..line.len() - 1]
        .parse::<usize>()
        .map(Some)
        .map_err(|_| toonl_error(line_number, "invalid trailer count"))
}

fn parse_toonl_row(
    line: &str,
    delimiter: char,
    expected_cells: usize,
    line_number: usize,
) -> Result<Vec<String>, ToonlError> {
    let row =
        split_delimited(line, delimiter, line_number).map_err(ToonlError::from_parse_error)?;
    if row.len() != expected_cells {
        return Err(toonl_error(line_number, "row arity mismatch"));
    }
    for cell in &row {
        parse_scalar(cell, line_number).map_err(ToonlError::from_parse_error)?;
    }
    Ok(row)
}

fn toonl_row_value(fields: &[String], row: &[String], line: usize) -> Result<Value, ToonlError> {
    let fields = fields
        .iter()
        .zip(row)
        .map(|(key, cell)| {
            Ok(Field {
                key: key.clone(),
                value: parse_scalar(cell, line).map_err(ToonlError::from_parse_error)?,
            })
        })
        .collect::<Result<Vec<_>, ToonlError>>()?;
    Ok(Value::Object(Document { fields }))
}

fn validate_toonl_delimiter(delimiter: char) -> Result<(), ToonlError> {
    if matches!(delimiter, DOCUMENT_DELIMITER | '|' | '\t') {
        Ok(())
    } else {
        Err(toonl_error(0, "invalid header delimiter"))
    }
}

fn toonl_value_fields(value: &Value) -> Result<Vec<String>, ToonlError> {
    let Value::Object(document) = value else {
        return Err(toonl_error(0, "TOONL output requires object rows"));
    };
    if document.fields.is_empty() {
        return Err(toonl_error(0, "TOONL output requires object rows"));
    }
    for field in &document.fields {
        if !field.value.is_primitive() {
            return Err(toonl_error(0, "TOONL rows must be flat objects"));
        }
    }
    Ok(document
        .fields
        .iter()
        .map(|field| field.key.clone())
        .collect())
}

fn toonl_value_cells(
    value: &Value,
    fields: &[String],
    delimiter: char,
) -> Result<Vec<String>, ToonlError> {
    let Value::Object(document) = value else {
        return Err(toonl_error(0, "TOONL output requires object rows"));
    };
    let mut cells = Vec::with_capacity(fields.len());
    for field in fields {
        let Some(value) = document.get(field) else {
            return Err(toonl_error(0, "TOONL output schema changed"));
        };
        if !value.is_primitive() {
            return Err(toonl_error(0, "TOONL rows must be flat objects"));
        }
        cells.push(primitive_text(value, delimiter));
    }
    Ok(cells)
}

fn toonl_shape_key(fields: &[String]) -> Vec<String> {
    let mut key = fields.to_vec();
    key.sort();
    key
}

fn canonical_toonl_fields(
    fields: Vec<String>,
    fields_by_shape: &mut BTreeMap<Vec<String>, Vec<String>>,
) -> Vec<String> {
    let key = toonl_shape_key(&fields);
    if let Some(canonical) = fields_by_shape.get(&key) {
        return canonical.clone();
    }
    fields_by_shape.insert(key, fields.clone());
    fields
}

fn toonl_error(line: usize, message: impl Into<String>) -> ToonlError {
    ToonlError {
        line,
        message: message.into(),
    }
}

fn read_toonl_error(error: std::io::Error) -> ToonlError {
    toonl_error(0, format!("read error: {error}"))
}

fn write_toonl_error(error: std::io::Error) -> ToonlError {
    toonl_error(0, format!("write error: {error}"))
}

// ---------------------------------------------------------------------------
// Field insertion, duplicate keys and path expansion
// ---------------------------------------------------------------------------




// ---------------------------------------------------------------------------
// Scalars, strings and keys
// ---------------------------------------------------------------------------

fn parse_scalar(value: &str, line: usize) -> Result<Value, ParseError> {
    if value.is_empty() {
        return Ok(Value::String(String::new()));
    }

    if value.starts_with('"') {
        return parse_quoted_string(value, line).map(Value::String);
    }

    if value.contains('"') {
        return Err(ParseError {
            line,
            message: "invalid quoted string",
            max_depth: None,
        });
    }

    Ok(match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        value if is_number_token(value) => Value::Number(value.to_owned()),
        value => Value::String(value.to_owned()),
    })
}

fn parse_key(value: &str, line: usize) -> Result<(String, bool), ParseError> {
    let value = value.trim();
    if value.starts_with('"') {
        return parse_quoted_string(value, line).map(|key| (key, true));
    }
    if value.contains('"') || value.contains(char::is_whitespace) {
        return Err(ParseError {
            line,
            message: "expected non-empty field name",
            max_depth: None,
        });
    }
    Ok((value.to_owned(), false))
}

fn parse_quoted_string(value: &str, line: usize) -> Result<String, ParseError> {
    let mut characters = value.chars();
    if characters.next() != Some('"') {
        return Err(invalid_quoted_string(line));
    }

    let mut output = String::new();
    while let Some(character) = characters.next() {
        match character {
            '"' => {
                if characters.as_str().trim().is_empty() {
                    return Ok(output);
                }
                return Err(invalid_quoted_string(line));
            }
            '\\' => {
                let escaped = characters.next().ok_or(invalid_quoted_string(line))?;
                match escaped {
                    '"' => output.push('"'),
                    '\\' => output.push('\\'),
                    'n' => output.push('\n'),
                    'r' => output.push('\r'),
                    't' => output.push('\t'),
                    'u' => output.push(parse_unicode_escape(&mut characters, line)?),
                    _ => return Err(invalid_quoted_string(line)),
                }
            }
            // Literal HTAB is tolerated; other C0 controls must be escaped (§7.1).
            character if (character as u32) < 0x20 && character != '\t' => {
                return Err(invalid_quoted_string(line));
            }
            character => output.push(character),
        }
    }

    Err(invalid_quoted_string(line))
}

fn parse_unicode_escape(
    characters: &mut std::str::Chars<'_>,
    line: usize,
) -> Result<char, ParseError> {
    let mut value = 0;
    for _ in 0..4 {
        let character = characters.next().ok_or(invalid_quoted_string(line))?;
        value = value * 16 + character.to_digit(16).ok_or(invalid_quoted_string(line))?;
    }

    // `char::from_u32` rejects lone surrogates, which §7.1 requires.
    char::from_u32(value).ok_or(invalid_quoted_string(line))
}

fn invalid_quoted_string(line: usize) -> ParseError {
    ParseError {
        line,
        message: "invalid quoted string",
        max_depth: None,
    }
}

/// Splits on unquoted occurrences of `delimiter`, preserving empty tokens (§11.2).
fn split_delimited(value: &str, delimiter: char, line: usize) -> Result<Vec<String>, ParseError> {
    if value.is_empty() {
        return Ok(Vec::new());
    }

    let mut values = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            character if character == delimiter && !in_string => {
                values.push(value[start..index].trim().to_owned());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }

    if in_string || escaped {
        return Err(invalid_quoted_string(line));
    }

    values.push(value[start..].trim().to_owned());
    Ok(values)
}

fn find_unquoted(value: &str, needle: char, line: usize) -> Result<Option<usize>, ParseError> {
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            character if character == needle && !in_string => return Ok(Some(index)),
            _ => {}
        }
    }

    if in_string || escaped {
        return Err(invalid_quoted_string(line));
    }

    Ok(None)
}

