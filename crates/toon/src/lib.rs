//! TOON (Token-Oriented Object Notation) parser and serializer.
//!
//! Implements the v3.3 working draft hosted at <https://github.com/toon-format/spec>.
//! The decoder honours the spec's decoder options (`indent`, `strict`, `expandPaths`);
//! the encoder emits the canonical default profile: comma document delimiter,
//! two-space indentation, no key folding.

use std::fmt;

/// Spaces per indentation level unless [`ParseOptions::indent`] says otherwise.
pub const DEFAULT_INDENT: usize = 2;

/// The document delimiter used by the encoder (spec §11.1, default profile).
const DOCUMENT_DELIMITER: char = ',';

/// Decoder options (spec §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// Spaces per indentation level.
    pub indent: usize,
    /// Enforce the §14 strict-mode error checklist.
    pub strict: bool,
    /// Expand dotted keys into nested objects (spec §13.4, `expandPaths: "safe"`).
    pub expand_paths: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            indent: DEFAULT_INDENT,
            strict: true,
            expand_paths: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    key: String,
    value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    line: usize,
    message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Array(Array),
    Bool(bool),
    Null,
    Number(String),
    Object(Document),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Array {
    List(Vec<Value>),
    Tabular(TabularArray),
}

/// An array of uniform objects kept in row form so untouched rows are never
/// materialised into [`Document`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabularArray {
    fields: Vec<String>,
    rows: Vec<Vec<Value>>,
}

#[derive(Debug)]
struct Line<'a> {
    number: usize,
    depth: usize,
    content: &'a str,
    /// A blank line separates this line from the previous non-blank one.
    blank_before: bool,
}

#[derive(Debug)]
struct Header {
    key: String,
    key_quoted: bool,
    len: usize,
    delimiter: char,
    fields: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl Document {
    /// Parses a document whose root is an object.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        Self::parse_with_options(input, ParseOptions::default())
    }

    pub fn parse_with_options(input: &str, options: ParseOptions) -> Result<Self, ParseError> {
        match Value::parse_with_options(input, options)? {
            Value::Object(document) => Ok(document),
            _ => Err(ParseError {
                line: 1,
                message: "expected `key: value`",
            }),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| &field.value)
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn to_canonical_toon(&self) -> String {
        let mut output = String::new();
        self.write_fields(&mut output, 0);
        output
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for field in &self.fields {
            map.insert(field.key.clone(), field.value.to_json_value());
        }
        serde_json::Value::Object(map)
    }

    fn write_fields(&self, output: &mut String, depth: usize) {
        for field in &self.fields {
            write_indent(output, depth);
            write_field(output, &field.key, &field.value, depth);
        }
    }
}

impl ParseError {
    pub fn line(&self) -> usize {
        self.line
    }

    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

impl Value {
    pub fn parse_toon(input: &str) -> Result<Self, ParseError> {
        Self::parse_with_options(input, ParseOptions::default())
    }

    /// Decodes TOON per spec §5 root-form discovery.
    pub fn parse_with_options(input: &str, options: ParseOptions) -> Result<Self, ParseError> {
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
            });
        }

        let only_line = lines.len() == 1;
        if only_line && first.content.trim() == "[]" {
            return Ok(Self::Array(Array::List(Vec::new())));
        }

        if first.content.starts_with('[') {
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
            });
        }
        Ok(Self::Object(document))
    }

    pub fn from_json_str(input: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(input).map(Self::from_json_value)
    }

    pub fn from_json_value(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Array(values) => Self::Array(Array::List(
                values.into_iter().map(Self::from_json_value).collect(),
            )),
            serde_json::Value::Bool(value) => Self::Bool(value),
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Number(value) => Self::Number(value.to_string()),
            serde_json::Value::Object(map) => {
                let fields = map
                    .into_iter()
                    .map(|(key, value)| Field {
                        key,
                        value: Self::from_json_value(value),
                    })
                    .collect();
                Self::Object(Document { fields })
            }
            serde_json::Value::String(value) => Self::String(value),
        }
    }

    pub fn to_canonical_toon(&self) -> String {
        let mut output = String::new();
        match self {
            Self::Array(array) => write_array(&mut output, None, &array.values(), 0, false),
            Self::Object(document) => document.write_fields(&mut output, 0),
            value => output.push_str(&primitive_text(value, DOCUMENT_DELIMITER)),
        }
        output
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Self::Array(array) => array.to_json_value(),
            Self::Bool(value) => serde_json::Value::Bool(*value),
            Self::Null => serde_json::Value::Null,
            Self::Number(value) => serde_json::from_str(&canonical_number(value))
                .ok()
                .filter(serde_json::Value::is_number)
                .unwrap_or_else(|| serde_json::Value::String(value.clone())),
            Self::Object(document) => document.to_json_value(),
            Self::String(value) => serde_json::Value::String(value.clone()),
        }
    }

    pub fn to_json_string(&self, compact: bool) -> Result<String, serde_json::Error> {
        let value = self.to_json_value();
        if compact {
            serde_json::to_string(&value)
        } else {
            serde_json::to_string_pretty(&value)
        }
    }

    pub fn as_object(&self) -> Option<&Document> {
        match self {
            Self::Object(document) => Some(document),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Array> {
        match self {
            Self::Array(array) => Some(array),
            _ => None,
        }
    }

    fn is_primitive(&self) -> bool {
        !matches!(self, Self::Array(_) | Self::Object(_))
    }
}

impl Array {
    pub fn len(&self) -> usize {
        match self {
            Self::List(values) => values.len(),
            Self::Tabular(table) => table.rows.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Option<Value> {
        match self {
            Self::List(values) => values.get(index).cloned(),
            Self::Tabular(table) => table.get(index),
        }
    }

    pub fn slice(&self, start: Option<usize>, end: Option<usize>) -> Self {
        let len = self.len();
        let start = start.unwrap_or(0).min(len);
        let end = end.unwrap_or(len).min(len).max(start);

        match self {
            Self::List(values) => Self::List(values[start..end].to_vec()),
            Self::Tabular(table) => Self::Tabular(TabularArray {
                fields: table.fields.clone(),
                rows: table.rows[start..end].to_vec(),
            }),
        }
    }

    pub fn values(&self) -> Vec<Value> {
        (0..self.len())
            .filter_map(|index| self.get(index))
            .collect()
    }

    pub fn to_canonical_toon(&self) -> String {
        let mut output = String::new();
        write_array(&mut output, None, &self.values(), 0, false);
        output
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.values()
                .into_iter()
                .map(|value| value.to_json_value())
                .collect(),
        )
    }
}

impl TabularArray {
    fn get(&self, index: usize) -> Option<Value> {
        self.rows.get(index).map(|row| {
            count_tabular_row_decode_for_tests();
            Value::Object(Document {
                fields: self
                    .fields
                    .iter()
                    .zip(row)
                    .map(|(key, value)| Field {
                        key: key.clone(),
                        value: value.clone(),
                    })
                    .collect(),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------

fn collect_lines<'a>(input: &'a str, options: &ParseOptions) -> Result<Vec<Line<'a>>, ParseError> {
    let mut lines = Vec::new();
    let mut blank_before = false;

    for (index, raw_line) in input.lines().enumerate() {
        let number = index + 1;
        let raw_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if raw_line.trim().is_empty() {
            blank_before = true;
            continue;
        }

        let spaces = raw_line.len() - raw_line.trim_start_matches(' ').len();
        if raw_line[spaces..].starts_with('\t') {
            return Err(ParseError {
                line: number,
                message: "invalid indentation",
            });
        }
        if options.strict && spaces % options.indent != 0 {
            return Err(ParseError {
                line: number,
                message: "invalid indentation",
            });
        }

        lines.push(Line {
            number,
            depth: spaces / options.indent,
            content: &raw_line[spaces..],
            blank_before,
        });
        blank_before = false;
    }

    Ok(lines)
}

// ---------------------------------------------------------------------------
// Objects
// ---------------------------------------------------------------------------

fn parse_object(
    lines: &[Line<'_>],
    index: &mut usize,
    depth: usize,
    options: &ParseOptions,
) -> Result<Document, ParseError> {
    let mut document = Document::default();

    while let Some(line) = lines.get(*index) {
        if line.depth < depth {
            break;
        }
        if line.depth > depth {
            return Err(ParseError {
                line: line.number,
                message: "invalid indentation",
            });
        }

        let (key, quoted, value) = parse_field(lines, index, depth, options)?;
        insert_field(&mut document, &key, quoted, value, options, line.number)?;
    }

    Ok(document)
}

/// Parses one `key: value` line (and any body it owns), advancing `index`.
fn parse_field(
    lines: &[Line<'_>],
    index: &mut usize,
    depth: usize,
    options: &ParseOptions,
) -> Result<(String, bool, Value), ParseError> {
    let line = &lines[*index];
    let content = line.content;
    let colon = find_unquoted(content, ':', line.number)?.ok_or(ParseError {
        line: line.number,
        message: "expected `key: value`",
    })?;
    let key_part = &content[..colon];
    let value_part = &content[colon + 1..];

    if find_unquoted(key_part, '[', line.number)?.is_some() {
        match parse_header(key_part, Some(colon)) {
            Ok(header) => {
                if header.key.is_empty() && !header.key_quoted {
                    return Err(ParseError {
                        line: line.number,
                        message: "expected non-empty field name",
                    });
                }
                let key = header.key.clone();
                let value = parse_array_field(&header, value_part, lines, index, depth, options)?;
                return Ok((key, header.key_quoted, value));
            }
            Err(error) if options.strict => return Err(error.at(line.number)),
            // Non-strict decoders fall through to key-value parsing with the
            // whole prefix as a literal key (spec §6).
            Err(_) => {
                *index += 1;
                let value =
                    parse_field_value(lines, index, depth, value_part, line.number, options)?;
                return Ok((key_part.trim().to_owned(), false, value));
            }
        }
    }

    let (key, quoted) = parse_key(key_part, line.number)?;
    if key.is_empty() && !quoted {
        return Err(ParseError {
            line: line.number,
            message: "expected non-empty field name",
        });
    }
    *index += 1;
    let value = parse_field_value(lines, index, depth, value_part, line.number, options)?;
    Ok((key, quoted, value))
}

/// Value of a non-header field. `index` already points past the field's own line.
fn parse_field_value(
    lines: &[Line<'_>],
    index: &mut usize,
    depth: usize,
    value_part: &str,
    line: usize,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    let text = value_part.trim();
    if text == "[]" {
        return Ok(Value::Array(Array::List(Vec::new())));
    }
    if !text.is_empty() {
        return parse_scalar(text, line);
    }

    // A bare `key:` opens a nested — possibly empty — object, never an array (§8).
    match lines.get(*index) {
        Some(next) if next.depth > depth => Ok(Value::Object(parse_object(
            lines,
            index,
            depth + 1,
            options,
        )?)),
        _ => Ok(Value::Object(Document::default())),
    }
}

// ---------------------------------------------------------------------------
// Arrays
// ---------------------------------------------------------------------------

fn parse_root_array(
    header: Header,
    lines: &[Line<'_>],
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    let first = &lines[0];
    let colon = find_unquoted(first.content, ':', first.number)?.ok_or(ParseError {
        line: first.number,
        message: "expected `key: value`",
    })?;
    let value_part = &first.content[colon + 1..];
    let mut index = 0;
    let value = parse_array_field(&header, value_part, lines, &mut index, 0, options)?;
    if let Some(line) = lines.get(index) {
        return Err(ParseError {
            line: line.number,
            message: "expected end of document",
        });
    }
    Ok(value)
}

/// Reads an array declared by `header`; `index` points at the header line.
fn parse_array_field(
    header: &Header,
    value_part: &str,
    lines: &[Line<'_>],
    index: &mut usize,
    header_depth: usize,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    let header_line = lines[*index].number;
    let inline = value_part.trim();
    *index += 1;

    if let Some(fields) = &header.fields {
        if !inline.is_empty() {
            return Err(ParseError {
                line: header_line,
                message: "expected tabular rows",
            });
        }
        return parse_tabular_rows(header, fields, lines, index, header_depth + 1, options);
    }

    if !inline.is_empty() {
        let values = split_delimited(value_part.trim(), header.delimiter, header_line)?
            .iter()
            .map(|value| parse_scalar(value, header_line))
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != header.len {
            return Err(ParseError {
                line: header_line,
                message: "array length mismatch",
            });
        }
        return Ok(Value::Array(Array::List(values)));
    }

    parse_list_items(header, lines, index, header_depth + 1, options)
}

fn parse_tabular_rows(
    header: &Header,
    fields: &[String],
    lines: &[Line<'_>],
    index: &mut usize,
    row_depth: usize,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    let mut rows = Vec::new();

    while rows.len() < header.len {
        let Some(line) = lines.get(*index) else {
            return Err(length_mismatch(lines, *index));
        };
        if line.depth < row_depth {
            break;
        }
        if line.depth > row_depth {
            return Err(ParseError {
                line: line.number,
                message: "invalid indentation",
            });
        }
        if line.blank_before && options.strict {
            return Err(ParseError {
                line: line.number,
                message: "blank line inside array",
            });
        }
        if !is_tabular_row(line.content, header.delimiter, line.number)? {
            break;
        }

        let cells = split_delimited(line.content, header.delimiter, line.number)?;
        if cells.len() != fields.len() {
            return Err(ParseError {
                line: line.number,
                message: "array row length mismatch",
            });
        }
        rows.push(
            cells
                .iter()
                .map(|cell| parse_scalar(cell, line.number))
                .collect::<Result<Vec<_>, _>>()?,
        );
        *index += 1;
    }

    if rows.len() != header.len {
        return Err(length_mismatch(lines, *index));
    }
    if let Some(line) = lines.get(*index) {
        if line.depth >= row_depth && is_tabular_row(line.content, header.delimiter, line.number)? {
            return Err(ParseError {
                line: line.number,
                message: "array length mismatch",
            });
        }
    }

    Ok(Value::Array(Array::Tabular(TabularArray {
        fields: fields.to_vec(),
        rows,
    })))
}

/// Spec §9.3 row disambiguation: a same-depth line is a row unless an unquoted
/// colon precedes the first unquoted active delimiter.
fn is_tabular_row(content: &str, delimiter: char, line: usize) -> Result<bool, ParseError> {
    let Some(colon) = find_unquoted(content, ':', line)? else {
        return Ok(true);
    };
    match find_unquoted(content, delimiter, line)? {
        Some(delimiter_index) => Ok(delimiter_index < colon),
        None => Ok(false),
    }
}

fn parse_list_items(
    header: &Header,
    lines: &[Line<'_>],
    index: &mut usize,
    item_depth: usize,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    let mut values = Vec::new();

    while values.len() < header.len {
        let Some(line) = lines.get(*index) else {
            return Err(length_mismatch(lines, *index));
        };
        if line.depth < item_depth {
            return Err(length_mismatch(lines, *index));
        }
        if line.depth > item_depth {
            return Err(ParseError {
                line: line.number,
                message: "invalid indentation",
            });
        }
        if line.blank_before && options.strict {
            return Err(ParseError {
                line: line.number,
                message: "blank line inside array",
            });
        }
        values.push(parse_list_item(lines, index, item_depth, options)?);
    }

    if let Some(line) = lines.get(*index) {
        if line.depth >= item_depth {
            return Err(ParseError {
                line: line.number,
                message: "array length mismatch",
            });
        }
    }

    Ok(Value::Array(Array::List(values)))
}

fn parse_list_item(
    lines: &[Line<'_>],
    index: &mut usize,
    item_depth: usize,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    let line = &lines[*index];
    let Some(rest) = line.content.strip_prefix('-') else {
        return Err(ParseError {
            line: line.number,
            message: "expected array item",
        });
    };
    let inner = rest.trim_start();

    // Bare `-`: an empty object list item (§10).
    if inner.is_empty() {
        *index += 1;
        return Ok(Value::Object(Document::default()));
    }

    // `- [M]: …`: a nested array whose body sits one level under the hyphen (§9.4).
    if inner.starts_with('[') {
        let colon = find_unquoted(inner, ':', line.number)?;
        if let Ok(header) = parse_header(inner, colon) {
            let colon = colon.expect("a parsed header ends at its colon");
            let value_part = inner[colon + 1..].to_owned();
            return parse_array_field(&header, &value_part, lines, index, item_depth, options);
        }
    }

    // `- key: …`: an object whose fields live at the hyphen's content column.
    if find_unquoted(inner, ':', line.number)?.is_some() {
        let mut item_lines = vec![Line {
            number: line.number,
            depth: item_depth + 1,
            content: inner,
            blank_before: false,
        }];
        *index += 1;
        while let Some(next) = lines.get(*index) {
            if next.depth <= item_depth {
                break;
            }
            item_lines.push(Line {
                number: next.number,
                depth: next.depth,
                content: next.content,
                blank_before: next.blank_before,
            });
            *index += 1;
        }

        let mut item_index = 0;
        let document = parse_object(&item_lines, &mut item_index, item_depth + 1, options)?;
        return Ok(Value::Object(document));
    }

    *index += 1;
    parse_scalar(inner, line.number)
}

fn length_mismatch(lines: &[Line<'_>], index: usize) -> ParseError {
    let line = lines
        .get(index)
        .or_else(|| lines.last())
        .map_or(1, |line| line.number);
    ParseError {
        line,
        message: "array length mismatch",
    }
}

// ---------------------------------------------------------------------------
// Headers
// ---------------------------------------------------------------------------

/// A header error before its line number is known.
struct HeaderError(&'static str);

impl HeaderError {
    fn at(self, line: usize) -> ParseError {
        ParseError {
            line,
            message: self.0,
        }
    }
}

/// Parses `key[N<delim?>]{fields}:` (spec §6). `colon` is the first unquoted
/// colon on the line; the header must terminate exactly there.
fn parse_header(content: &str, colon: Option<usize>) -> Result<Header, HeaderError> {
    let colon = colon.ok_or(HeaderError("array header missing colon"))?;
    let key_part = &content[..colon];

    let open = find_unquoted(key_part, '[', 0)
        .map_err(|_| HeaderError("invalid quoted string"))?
        .ok_or(HeaderError("invalid array header"))?;
    let (key, key_quoted) =
        parse_key(&key_part[..open], 0).map_err(|_| HeaderError("invalid array header"))?;

    let rest = &key_part[open + 1..];
    let close = rest.find(']').ok_or(HeaderError("invalid array header"))?;
    let bracket = &rest[..close];

    let digits = bracket
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
        return Err(HeaderError("invalid array header"));
    }
    let len = digits
        .parse()
        .map_err(|_| HeaderError("invalid array header"))?;
    let delimiter = match &bracket[digits.len()..] {
        "" => ',',
        "\t" => '\t',
        "|" => '|',
        _ => return Err(HeaderError("invalid array header")),
    };

    let suffix = &rest[close + 1..];
    let fields = if suffix.is_empty() {
        None
    } else if suffix.starts_with('{') && suffix.ends_with('}') && suffix.len() >= 2 {
        let names = split_delimited(&suffix[1..suffix.len() - 1], delimiter, 0)
            .map_err(|_| HeaderError("invalid array header"))?;
        Some(
            names
                .iter()
                .map(|name| {
                    parse_key(name, 0)
                        .map(|(key, _)| key)
                        .map_err(|_| HeaderError("invalid array header"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        return Err(HeaderError("invalid array header"));
    };

    Ok(Header {
        key,
        key_quoted,
        len,
        delimiter,
        fields,
    })
}

// ---------------------------------------------------------------------------
// Field insertion, duplicate keys and path expansion
// ---------------------------------------------------------------------------

fn insert_field(
    document: &mut Document,
    key: &str,
    quoted: bool,
    value: Value,
    options: &ParseOptions,
    line: usize,
) -> Result<(), ParseError> {
    if options.expand_paths && !quoted && key.contains('.') {
        let segments = key.split('.').collect::<Vec<_>>();
        if segments
            .iter()
            .all(|segment| is_identifier_segment(segment))
        {
            return insert_path(document, &segments, value, options, line);
        }
    }

    insert_path(document, &[key], value, options, line)
}

fn insert_path(
    document: &mut Document,
    segments: &[&str],
    value: Value,
    options: &ParseOptions,
    line: usize,
) -> Result<(), ParseError> {
    let key = segments[0];
    let existing = document.fields.iter().position(|field| field.key == key);

    if segments.len() == 1 {
        match existing {
            Some(_) if options.strict => Err(ParseError {
                line,
                message: "duplicate key",
            }),
            // Last write wins in non-strict mode (§14.3, §14.4).
            Some(position) => {
                document.fields[position].value = value;
                Ok(())
            }
            None => {
                document.fields.push(Field {
                    key: key.to_owned(),
                    value,
                });
                Ok(())
            }
        }
    } else {
        let position = match existing {
            Some(position) => {
                if !matches!(document.fields[position].value, Value::Object(_)) {
                    if options.strict {
                        return Err(ParseError {
                            line,
                            message: "path expansion conflict",
                        });
                    }
                    document.fields[position].value = Value::Object(Document::default());
                }
                position
            }
            None => {
                document.fields.push(Field {
                    key: key.to_owned(),
                    value: Value::Object(Document::default()),
                });
                document.fields.len() - 1
            }
        };

        let Value::Object(nested) = &mut document.fields[position].value else {
            unreachable!("the branch above guarantees an object");
        };
        insert_path(nested, &segments[1..], value, options, line)
    }
}

fn is_identifier_segment(segment: &str) -> bool {
    let mut characters = segment.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

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

fn write_indent(output: &mut String, depth: usize) {
    for _ in 0..depth * DEFAULT_INDENT {
        output.push(' ');
    }
}

/// Writes `key: value` at the caller's cursor (indent or `- ` already emitted).
fn write_field(output: &mut String, key: &str, value: &Value, depth: usize) {
    match value {
        Value::Array(array) => write_array(output, Some(key), &array.values(), depth, false),
        Value::Object(document) => {
            output.push_str(&canonical_key(key));
            output.push_str(":\n");
            document.write_fields(output, depth + 1);
        }
        value => {
            output.push_str(&canonical_key(key));
            output.push_str(": ");
            output.push_str(&primitive_text(value, DOCUMENT_DELIMITER));
            output.push('\n');
        }
    }
}

/// Writes an array header plus body. `list_item` selects the empty-array form:
/// `[0]:` inside a list (§9.2) versus `key: []` / `[]` elsewhere (§9.1).
fn write_array(
    output: &mut String,
    key: Option<&str>,
    values: &[Value],
    depth: usize,
    list_item: bool,
) {
    if values.is_empty() {
        match key {
            Some(key) => {
                output.push_str(&canonical_key(key));
                output.push_str(": []\n");
            }
            None if list_item => output.push_str("[0]:\n"),
            None => output.push_str("[]\n"),
        }
        return;
    }

    if values.iter().all(Value::is_primitive) {
        write_array_header(output, key, values.len(), None);
        output.push(' ');
        let cells = values
            .iter()
            .map(|value| primitive_text(value, DOCUMENT_DELIMITER))
            .collect::<Vec<_>>();
        output.push_str(&cells.join(&DOCUMENT_DELIMITER.to_string()));
        output.push('\n');
        return;
    }

    // In list-item position the tabular form has nowhere to put its field list,
    // so §9.4 requires the expanded list even for a uniform array of objects.
    if let Some(fields) = tabular_fields(values).filter(|_| !list_item) {
        write_array_header(output, key, values.len(), Some(&fields));
        output.push('\n');
        for value in values {
            let Value::Object(document) = value else {
                unreachable!("tabular_fields only matches objects");
            };
            write_indent(output, depth + 1);
            let cells = fields
                .iter()
                .map(|field| {
                    let cell = document.get(field).expect("tabular_fields checked the key");
                    primitive_text(cell, DOCUMENT_DELIMITER)
                })
                .collect::<Vec<_>>();
            output.push_str(&cells.join(&DOCUMENT_DELIMITER.to_string()));
            output.push('\n');
        }
        return;
    }

    write_array_header(output, key, values.len(), None);
    output.push('\n');
    for value in values {
        write_indent(output, depth + 1);
        write_list_item(output, value, depth + 1);
    }
}

fn write_list_item(output: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Object(document) if document.fields.is_empty() => output.push_str("-\n"),
        Value::Object(document) => {
            output.push_str("- ");
            let first = &document.fields[0];
            write_field(output, &first.key, &first.value, depth + 1);
            for field in &document.fields[1..] {
                write_indent(output, depth + 1);
                write_field(output, &field.key, &field.value, depth + 1);
            }
        }
        Value::Array(array) => {
            output.push_str("- ");
            write_array(output, None, &array.values(), depth, true);
        }
        value => {
            output.push_str("- ");
            output.push_str(&primitive_text(value, DOCUMENT_DELIMITER));
            output.push('\n');
        }
    }
}

fn write_array_header(
    output: &mut String,
    key: Option<&str>,
    len: usize,
    fields: Option<&[String]>,
) {
    if let Some(key) = key {
        output.push_str(&canonical_key(key));
    }
    output.push('[');
    output.push_str(&len.to_string());
    output.push(']');
    if let Some(fields) = fields {
        output.push('{');
        let names = fields
            .iter()
            .map(|field| canonical_key(field))
            .collect::<Vec<_>>();
        output.push_str(&names.join(&DOCUMENT_DELIMITER.to_string()));
        output.push('}');
    }
    output.push(':');
}

/// Tabular eligibility (§9.3): every element is a non-empty object, all share the
/// first element's key set, and every value is primitive.
fn tabular_fields(values: &[Value]) -> Option<Vec<String>> {
    let Value::Object(first) = values.first()? else {
        return None;
    };
    if first.fields.is_empty() {
        return None;
    }
    let fields = first
        .fields
        .iter()
        .map(|field| field.key.clone())
        .collect::<Vec<_>>();

    for value in values {
        let Value::Object(document) = value else {
            return None;
        };
        if document.fields.len() != fields.len() {
            return None;
        }
        if document
            .fields
            .iter()
            .any(|field| !field.value.is_primitive())
        {
            return None;
        }
        if fields.iter().any(|field| document.get(field).is_none()) {
            return None;
        }
    }

    Some(fields)
}

fn primitive_text(value: &Value, delimiter: char) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_owned(),
        Value::Number(value) => canonical_number(value),
        Value::String(value) => canonical_string(value, delimiter),
        Value::Array(_) | Value::Object(_) => unreachable!("not a primitive"),
    }
}

fn canonical_key(value: &str) -> String {
    if is_bare_key(value) {
        value.to_owned()
    } else {
        quote_string(value)
    }
}

/// Unquoted keys must match `^[A-Za-z_][A-Za-z0-9_.]*$` (§7.3).
fn is_bare_key(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.'))
}

fn canonical_string(value: &str, delimiter: char) -> String {
    if needs_quotes(value, delimiter) {
        quote_string(value)
    } else {
        value.to_owned()
    }
}

/// The §7.2 quoting checklist.
fn needs_quotes(value: &str, delimiter: char) -> bool {
    value.is_empty()
        || value.trim() != value
        || matches!(value, "true" | "false" | "null")
        || is_numeric_like(value)
        || value.contains([':', '"', '\\', '[', ']', '{', '}'])
        || value.chars().any(|character| (character as u32) < 0x20)
        || value.contains(delimiter)
        || value.starts_with('-')
}

fn quote_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

// ---------------------------------------------------------------------------
// Lazy-row instrumentation
// ---------------------------------------------------------------------------

#[cfg(test)]
static TABULAR_ROW_DECODE_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn count_tabular_row_decode_for_tests() {
    #[cfg(test)]
    {
        TABULAR_ROW_DECODE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
fn reset_tabular_row_decode_count_for_tests() {
    TABULAR_ROW_DECODE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
fn tabular_row_decode_count_for_tests() -> usize {
    TABULAR_ROW_DECODE_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::{Document, Value};

    #[test]
    fn parses_flat_fields_and_serializes_canonical_toon() {
        let document = Document::parse("name : Ada\nactive: true\ncount: 3\n").unwrap();

        assert_eq!(
            document.to_canonical_toon(),
            "name: Ada\nactive: true\ncount: 3\n"
        );
    }

    #[test]
    fn returns_top_level_value_by_name() {
        let document = Document::parse("name: Ada\n").unwrap();

        assert_eq!(
            document.get("name").unwrap().to_canonical_toon(),
            "Ada".to_owned()
        );
    }

    #[test]
    fn parses_nested_objects() {
        let document = Document::parse("person:\n  address:\n    city: London\n").unwrap();

        let person = document.get("person").and_then(Value::as_object).unwrap();
        let address = person.get("address").and_then(Value::as_object).unwrap();

        assert_eq!(
            address.get("city").unwrap().to_canonical_toon(),
            "London".to_owned()
        );
    }

    #[test]
    fn rejects_scalar_children() {
        let error = Document::parse("person: Ada\n  city: London\n").unwrap_err();

        assert_eq!(error.line(), 2);
        assert_eq!(error.message(), "invalid indentation");
    }

    #[test]
    fn parses_inline_list_arrays_and_serializes_canonical_toon() {
        let document = Document::parse("tags[3]: admin,ops,dev\n").unwrap();

        assert_eq!(document.to_canonical_toon(), "tags[3]: admin,ops,dev\n");
    }

    #[test]
    fn parses_tabular_arrays_and_serializes_canonical_toon() {
        let document =
            Document::parse("users[2]{id,name,active}:\n  1,Ada,true\n  2,\"Bob Smith\",false\n")
                .unwrap();

        assert_eq!(
            document.to_canonical_toon(),
            "users[2]{id,name,active}:\n  1,Ada,true\n  2,Bob Smith,false\n"
        );
    }

    #[test]
    fn rejects_array_length_mismatches() {
        let error = Document::parse("tags[2]: admin,ops,dev\n").unwrap_err();

        assert_eq!(error.line(), 1);
        assert_eq!(error.message(), "array length mismatch");
    }

    #[test]
    fn decodes_only_touched_tabular_rows() {
        let document =
            Document::parse("users[3]{id,name}:\n  1,Ada\n  2,Bob\n  3,Chloe\n").unwrap();
        let users = document.get("users").and_then(Value::as_array).unwrap();

        super::reset_tabular_row_decode_count_for_tests();
        let row = users.get(1).unwrap();

        assert_eq!(row.to_canonical_toon(), "id: 2\nname: Bob\n");
        assert_eq!(super::tabular_row_decode_count_for_tests(), 1);
    }
}
