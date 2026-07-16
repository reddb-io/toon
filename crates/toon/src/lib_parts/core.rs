use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::{BufRead, Write};

/// Spaces per indentation level unless [`ParseOptions::indent`] says otherwise.
pub const DEFAULT_INDENT: usize = 2;

/// Default maximum nesting depth for decoding and fallible encoding.
///
/// A value of `0` in [`ParseOptions::max_depth`] or [`EncodeOptions::max_depth`]
/// disables the guard for trusted input.
pub const DEFAULT_MAX_DEPTH: usize = 1000;

/// The default document delimiter used by the encoder (spec §11.1).
const DOCUMENT_DELIMITER: char = ',';
const CYCLIC_TABLE_DELIMITER: char = '|';
const TOONL_TAGGED_LANE_LIMIT: usize = 8;

/// Decoder options (spec §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// Spaces per indentation level.
    pub indent: usize,
    /// Enforce the §14 strict-mode error checklist.
    pub strict: bool,
    /// Expand dotted keys into nested objects (spec §13.4, `expandPaths: "safe"`).
    pub expand_paths: bool,
    /// Recognize the tabular cyclic discriminated-array extension during decode.
    pub cyclic_discriminated_arrays: bool,
    /// Maximum nesting depth. `0` disables the guard for trusted input.
    pub max_depth: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            indent: DEFAULT_INDENT,
            strict: true,
            expand_paths: false,
            cyclic_discriminated_arrays: true,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

/// Encoder options. Defaults preserve the canonical v3 output profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    /// Emit recursive-brace tabular headers for uniform nested object fields.
    pub nested_tabular_headers: bool,
    /// Emit brace-header tabular rows for keyed maps with uniform object values.
    pub keyed_map_collapse: bool,
    /// Emit primitive-array columns inside otherwise tabular object arrays.
    pub primitive_array_columns: bool,
    /// Emit child tables for array-valued columns inside tabular object arrays.
    pub object_array_columns: bool,
    /// Emit cyclic discriminated-array wire for strongly repeated event streams.
    pub cyclic_discriminated_arrays: bool,
    /// Active delimiter for encoded array and tabular rows: comma, pipe, or tab.
    pub delimiter: char,
    /// Maximum nesting depth for fallible encoding. `0` disables the guard.
    pub max_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncationKind {
    Complete,
    ArrayLengthMismatch,
    UnterminatedNesting,
    ToonlTrailerCountMismatch,
    ToonlMissingTrailer,
    Invalid,
}

impl TruncationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::ArrayLengthMismatch => "array_length_mismatch",
            Self::UnterminatedNesting => "unterminated_nesting",
            Self::ToonlTrailerCountMismatch => "toonl_trailer_count_mismatch",
            Self::ToonlMissingTrailer => "toonl_missing_trailer",
            Self::Invalid => "invalid",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationReport {
    pub complete: bool,
    pub kind: TruncationKind,
    pub line: Option<usize>,
    pub declared: Option<usize>,
    pub actual: Option<usize>,
    pub message: Option<String>,
}

impl TruncationReport {
    pub fn complete() -> Self {
        Self {
            complete: true,
            kind: TruncationKind::Complete,
            line: None,
            declared: None,
            actual: None,
            message: None,
        }
    }

    fn truncated(
        kind: TruncationKind,
        line: usize,
        declared: Option<usize>,
        actual: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            complete: false,
            kind,
            line: Some(line),
            declared,
            actual,
            message: Some(message.into()),
        }
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert(
            "complete".to_owned(),
            serde_json::Value::Bool(self.complete),
        );
        map.insert(
            "kind".to_owned(),
            serde_json::Value::String(self.kind.as_str().to_owned()),
        );
        map.insert(
            "line".to_owned(),
            self.line
                .map_or(serde_json::Value::Null, |value| value.into()),
        );
        map.insert(
            "declared".to_owned(),
            self.declared
                .map_or(serde_json::Value::Null, |value| value.into()),
        );
        map.insert(
            "actual".to_owned(),
            self.actual
                .map_or(serde_json::Value::Null, |value| value.into()),
        );
        map.insert(
            "message".to_owned(),
            self.message
                .as_ref()
                .map_or(serde_json::Value::Null, |value| value.clone().into()),
        );
        serde_json::Value::Object(map)
    }
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            nested_tabular_headers: false,
            keyed_map_collapse: false,
            primitive_array_columns: false,
            object_array_columns: false,
            cyclic_discriminated_arrays: false,
            delimiter: DOCUMENT_DELIMITER,
            max_depth: DEFAULT_MAX_DEPTH,
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
    max_depth: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeError {
    message: &'static str,
    max_depth: Option<usize>,
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
    fields: Vec<HeaderField>,
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
    fields: Option<Vec<HeaderField>>,
    field_tree: Option<Vec<HeaderFieldTree>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeaderField {
    path: Vec<String>,
    list_delimiter: Option<char>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeaderFieldTree {
    key: String,
    list_delimiter: Option<char>,
    fixed_len: Option<usize>,
    children: Vec<HeaderFieldTree>,
}

#[derive(Debug)]
struct MapHeader {
    key: String,
    key_quoted: bool,
    delimiter: char,
    fields: Vec<HeaderField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToonlError {
    line: usize,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToonlCursor {
    pub byte_offset: u64,
    pub active_header_line: String,
    pub rows_since_header: usize,
    pub anchor: Option<ToonlCursorAnchor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToonlCursorAnchor {
    pub byte_offset: u64,
    pub bytes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToonlCursorInvalidation {
    Truncated { byte_offset: u64, file_size: u64 },
    AnchorMismatch { byte_offset: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToonlResumeError {
    Invalid(ToonlCursorInvalidation),
    Parse(ToonlError),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToonlStream {
    segments: Vec<ToonlSegment>,
    interleaved_segments: Vec<ToonlSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToonlSegment {
    lane: Option<String>,
    delimiter: char,
    fields: Vec<String>,
    header_fields: String,
    rows: Vec<Vec<String>>,
}

/// A single TOONL segment with a fixed schema.
///
/// For multi-segment record streams, prefer [`encode_toonl_values`] or
/// [`ToonlWriter`]; those APIs canonicalize each record shape to the first field
/// order seen for that shape, as required by TOONL v0.2 R3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToonlEncoder {
    delimiter: char,
    fields: Vec<String>,
    header_fields: String,
    output: String,
    row_count: usize,
    rows_since_continuation: usize,
    bytes_since_continuation: usize,
    continuation_every_rows: Option<usize>,
    continuation_every_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenToonlSegment {
    delimiter: char,
    fields: Vec<String>,
    header_fields: String,
    row_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToonlHeaderLine {
    delimiter: char,
    fields: Vec<String>,
    header_fields: String,
    continuation: bool,
    tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaggedToonlLane {
    segments: Vec<ToonlSegment>,
    current: Option<ToonlSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ToonlLaneOrder {
    Anonymous,
    Tagged(String),
}

#[derive(Debug)]
pub struct ToonlRowReader<R> {
    reader: R,
    line: String,
    line_number: usize,
    byte_offset: u64,
    active_header_line: Option<String>,
    rows_since_header: usize,
    anchor: Option<ToonlCursorAnchor>,
    current: Option<OpenToonlSegment>,
    tagged_lanes: HashMap<String, OpenToonlSegment>,
    finished: bool,
}

pub type Record = Value;
pub type ToonlReader<R> = ToonlRowReader<R>;

/// Streaming TOONL writer for record values.
///
/// Field order is canonicalized per record shape using the first order seen for
/// that shape. Later records with the same field set but a different call-site
/// order reuse the original order and stay in the same segment when possible.
#[derive(Debug)]
pub struct ToonlWriter<W> {
    writer: W,
    delimiter: char,
    fields: Option<Vec<String>>,
    header_fields: Option<String>,
    fields_by_shape: BTreeMap<Vec<String>, Vec<String>>,
    tagged_lanes: HashMap<String, TaggedToonlWriterLane>,
    row_count: usize,
    rows_since_continuation: usize,
    bytes_since_continuation: usize,
    continuation_every_rows: Option<usize>,
    continuation_every_bytes: Option<usize>,
    finished: bool,
}

#[derive(Debug, Default)]
struct TaggedToonlWriterLane {
    fields: Option<Vec<String>>,
    fields_by_shape: BTreeMap<Vec<String>, Vec<String>>,
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
                max_depth: None,
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
        self.to_toon_with_options(EncodeOptions::default())
    }

    pub fn to_toon_with_options(&self, options: EncodeOptions) -> String {
        self.try_to_toon_with_options(options)
            .expect("TOON encoding failed")
    }

    pub fn try_to_canonical_toon(&self) -> Result<String, EncodeError> {
        self.try_to_toon_with_options(EncodeOptions::default())
    }

    pub fn try_to_toon_with_options(&self, options: EncodeOptions) -> Result<String, EncodeError> {
        let mut output = String::new();
        if write_cyclic_discriminated_arrays(&mut output, self, options)? {
            return Ok(output);
        }
        self.write_fields(&mut output, 0, options)?;
        Ok(output)
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for field in &self.fields {
            map.insert(field.key.clone(), field.value.to_json_value());
        }
        serde_json::Value::Object(map)
    }

    fn write_fields(
        &self,
        output: &mut String,
        depth: usize,
        options: EncodeOptions,
    ) -> Result<(), EncodeError> {
        check_encode_depth(depth, options)?;
        for field in &self.fields {
            write_indent(output, depth);
            write_field(output, &field.key, &field.value, depth, options)?;
        }
        Ok(())
    }
}

pub fn detect_truncation(input: &str) -> TruncationReport {
    detect_truncation_with_options(input, ParseOptions::default())
}

pub fn detect_truncation_with_options(input: &str, options: ParseOptions) -> TruncationReport {
    match Value::parse_with_options(input, options) {
        Ok(_) => TruncationReport::complete(),
        Err(error) if error.message() == "array length mismatch" => {
            detect_toon_array_truncation(input, options, error.line())
        }
        Err(error) => TruncationReport::truncated(
            TruncationKind::Invalid,
            error.line(),
            None,
            None,
            error.to_string(),
        ),
    }
}

pub fn detect_toonl_truncation(input: &str) -> TruncationReport {
    let mut open: Option<(usize, usize)> = None;
    for (offset, raw_line) in input.lines().enumerate() {
        let line_number = offset + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[=") && line.ends_with(']') {
            let declared = line[2..line.len() - 1].parse::<usize>().ok();
            let Some((_, actual)) = open.take() else {
                return TruncationReport::truncated(
                    TruncationKind::Invalid,
                    line_number,
                    declared,
                    None,
                    "trailer without header",
                );
            };
            if declared != Some(actual) {
                return TruncationReport::truncated(
                    TruncationKind::ToonlTrailerCountMismatch,
                    line_number,
                    declared,
                    Some(actual),
                    format!(
                        "trailer declared {} rows but received {actual}",
                        declared
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "invalid".to_owned())
                    ),
                );
            }
            continue;
        }
        if line.starts_with('[') && line.ends_with(':') && open.is_none() {
            open = Some((line_number, 0));
            continue;
        }
        if let Some((_, actual)) = open.as_mut() {
            *actual += 1;
        }
    }

    if let Some((line, actual)) = open {
        return TruncationReport::truncated(
            TruncationKind::ToonlMissingTrailer,
            line,
            None,
            Some(actual),
            format!("stream ended without a trailer after {actual} rows"),
        );
    }
    TruncationReport::complete()
}

fn detect_toon_array_truncation(
    input: &str,
    options: ParseOptions,
    fallback_line: usize,
) -> TruncationReport {
    let Ok(lines) = collect_lines(input, &options) else {
        return TruncationReport::truncated(
            TruncationKind::Invalid,
            fallback_line,
            None,
            None,
            "invalid indentation",
        );
    };

    for (index, line) in lines.iter().enumerate() {
        let Ok(Some(colon)) = find_unquoted(line.content, ':', line.number) else {
            continue;
        };
        if !line.content.contains('[') {
            continue;
        }
        let Ok(header) = parse_header(line.content, Some(colon)) else {
            continue;
        };
        let value_part = line.content[colon + 1..].trim();
        if header.fields.is_none() && !value_part.is_empty() {
            let actual = split_delimited(value_part, header.delimiter, line.number)
                .map(|values| values.len())
                .unwrap_or(0);
            if actual != header.len {
                return TruncationReport::truncated(
                    TruncationKind::ArrayLengthMismatch,
                    line.number,
                    Some(header.len),
                    Some(actual),
                    format!("declared {} items but received {actual}", header.len),
                );
            }
            continue;
        }

        let row_depth = line.depth + 1;
        let mut actual = 0;
        for row in lines.iter().skip(index + 1) {
            if row.depth < row_depth {
                break;
            }
            if row.depth == row_depth {
                actual += 1;
            }
        }
        if actual < header.len {
            let detected_line = lines.last().map_or(fallback_line, |line| line.number);
            return TruncationReport::truncated(
                TruncationKind::ArrayLengthMismatch,
                detected_line,
                Some(header.len),
                Some(actual),
                format!("declared {} rows but received {actual}", header.len),
            );
        }
    }

    TruncationReport::truncated(
        TruncationKind::UnterminatedNesting,
        lines.last().map_or(fallback_line, |line| line.number),
        None,
        None,
        "document ended before the declared nested structure was complete",
    )
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
        write!(formatter, "line {}: {}", self.line, self.message)?;
        if let Some(max_depth) = self.max_depth {
            write!(formatter, " (maxDepth {max_depth})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

impl EncodeError {
    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)?;
        if let Some(max_depth) = self.max_depth {
            write!(formatter, " (maxDepth {max_depth})")?;
        }
        Ok(())
    }
}

impl std::error::Error for EncodeError {}

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
        self.to_toon_with_options(EncodeOptions::default())
    }

    pub fn to_toon_with_options(&self, options: EncodeOptions) -> String {
        self.try_to_toon_with_options(options)
            .expect("TOON encoding failed")
    }

    pub fn try_to_canonical_toon(&self) -> Result<String, EncodeError> {
        self.try_to_toon_with_options(EncodeOptions::default())
    }

    pub fn try_to_toon_with_options(&self, options: EncodeOptions) -> Result<String, EncodeError> {
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

impl ToonlError {
    fn from_parse_error(error: ParseError) -> Self {
        Self {
            line: error.line,
            message: error.message.to_owned(),
        }
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ToonlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(formatter, "{}", self.message)
        } else {
            write!(formatter, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for ToonlError {}

impl ToonlCursor {
    pub fn new<T: Into<String>>(
        byte_offset: u64,
        active_header_line: T,
        rows_since_header: usize,
    ) -> Self {
        Self {
            byte_offset,
            active_header_line: active_header_line.into(),
            rows_since_header,
            anchor: None,
        }
    }

    pub fn to_json_string(&self) -> String {
        let mut object = serde_json::Map::new();
        object.insert("byteOffset".to_owned(), serde_json::json!(self.byte_offset));
        object.insert(
            "activeHeaderLine".to_owned(),
            serde_json::json!(self.active_header_line),
        );
        object.insert(
            "rowsSinceHeader".to_owned(),
            serde_json::json!(self.rows_since_header),
        );
        if let Some(anchor) = &self.anchor {
            object.insert(
                "anchor".to_owned(),
                serde_json::json!({
                    "byteOffset": anchor.byte_offset,
                    "bytes": anchor.bytes,
                }),
            );
        }
        serde_json::Value::Object(object).to_string()
    }

    pub fn from_json_str(input: &str) -> Result<Self, ToonlError> {
        let value: serde_json::Value = serde_json::from_str(input)
            .map_err(|error| toonl_error(0, format!("invalid cursor JSON: {error}")))?;
        let object = value
            .as_object()
            .ok_or_else(|| toonl_error(0, "invalid cursor JSON"))?;
        let byte_offset = object
            .get("byteOffset")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| toonl_error(0, "invalid cursor byteOffset"))?;
        let active_header_line = object
            .get("activeHeaderLine")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| toonl_error(0, "invalid cursor activeHeaderLine"))?
            .to_owned();
        let rows_since_header = object
            .get("rowsSinceHeader")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| toonl_error(0, "invalid cursor rowsSinceHeader"))?;
        let anchor = object
            .get("anchor")
            .map(|value| {
                let object = value
                    .as_object()
                    .ok_or_else(|| toonl_error(0, "invalid cursor anchor"))?;
                Ok(ToonlCursorAnchor {
                    byte_offset: object
                        .get("byteOffset")
                        .and_then(serde_json::Value::as_u64)
                        .ok_or_else(|| toonl_error(0, "invalid cursor anchor"))?,
                    bytes: object
                        .get("bytes")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| toonl_error(0, "invalid cursor anchor"))?
                        .to_owned(),
                })
            })
            .transpose()?;
        Ok(Self {
            byte_offset,
            active_header_line,
            rows_since_header,
            anchor,
        })
    }
}

impl fmt::Display for ToonlCursorInvalidation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { .. } => write!(formatter, "TOONL cursor invalidated by truncation"),
            Self::AnchorMismatch { .. } => {
                write!(formatter, "TOONL cursor invalidated by anchor mismatch")
            }
        }
    }
}

impl std::error::Error for ToonlCursorInvalidation {}

impl fmt::Display for ToonlResumeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => write!(formatter, "{error}"),
            Self::Parse(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ToonlResumeError {}

impl ToonlStream {
    pub fn parse(input: &str) -> Result<Self, ToonlError> {
        let mut anonymous_segments = Vec::new();
        let mut current: Option<ToonlSegment> = None;
        let mut tagged_lanes: HashMap<String, TaggedToonlLane> = HashMap::new();
        let mut lane_order: Vec<ToonlLaneOrder> = Vec::new();
        let mut anonymous_declared = false;
        let mut interleaved_segments = Vec::new();
        let mut saw_tagged_syntax = false;

        for (offset, raw_line) in input.lines().enumerate() {
            let line_number = offset + 1;
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            if line.is_empty() {
                continue;
            }
            if line.starts_with("- ") {
                return Err(toonl_error(line_number, "reserved line prefix"));
            }
            if let Some(expected) = toonl_trailer_count(line, line_number)? {
                let segment = current
                    .take()
                    .ok_or_else(|| toonl_error(line_number, "trailer without header"))?;
                if segment.rows.len() != expected {
                    return Err(toonl_error(line_number, "trailer count mismatch"));
                }
                anonymous_segments.push(segment);
                continue;
            }
            if let Some(header) = parse_toonl_header(line, line_number)? {
                if header.continuation {
                    ensure_continuation_matches(current.as_ref(), &header, line_number)?;
                    continue;
                }
                if let Some(tag) = header.tag {
                    saw_tagged_syntax = true;
                    let lane = if tagged_lanes.contains_key(&tag) {
                        tagged_lanes.get_mut(&tag).expect("lane exists")
                    } else {
                        if tagged_lanes.len() >= TOONL_TAGGED_LANE_LIMIT {
                            return Err(toonl_error(line_number, "too many tagged lanes"));
                        }
                        lane_order.push(ToonlLaneOrder::Tagged(tag.clone()));
                        tagged_lanes.insert(
                            tag.clone(),
                            TaggedToonlLane {
                                segments: Vec::new(),
                                current: None,
                            },
                        );
                        tagged_lanes.get_mut(&tag).expect("inserted lane exists")
                    };
                    if let Some(segment) = lane.current.take() {
                        lane.segments.push(segment);
                    }
                    lane.current = Some(ToonlSegment {
                        lane: Some(tag),
                        delimiter: header.delimiter,
                        fields: header.fields,
                        header_fields: header.header_fields,
                        rows: Vec::new(),
                    });
                    continue;
                }
                if let Some(segment) = current.take() {
                    anonymous_segments.push(segment);
                }
                if !anonymous_declared {
                    lane_order.push(ToonlLaneOrder::Anonymous);
                    anonymous_declared = true;
                }
                current = Some(ToonlSegment {
                    lane: None,
                    delimiter: header.delimiter,
                    fields: header.fields,
                    header_fields: header.header_fields,
                    rows: Vec::new(),
                });
                continue;
            }
            if let Some((tag, row_text)) = toonl_tagged_row_prefix(line, line_number)? {
                if let Some(lane) = tagged_lanes.get_mut(tag) {
                    saw_tagged_syntax = true;
                    let segment = lane
                        .current
                        .as_mut()
                        .expect("declared tagged lane has a current segment");
                    let row = parse_toonl_row(
                        row_text,
                        segment.delimiter,
                        segment.fields.len(),
                        line_number,
                    )?;
                    segment.rows.push(row.clone());
                    append_interleaved_toonl_row(&mut interleaved_segments, segment, row);
                    continue;
                }
                if current.is_none() {
                    return Err(toonl_error(line_number, "unknown tag"));
                }
            }

            let segment = current
                .as_mut()
                .ok_or_else(|| toonl_error(line_number, "row before header"))?;
            let row = parse_toonl_row(line, segment.delimiter, segment.fields.len(), line_number)?;
            segment.rows.push(row.clone());
            append_interleaved_toonl_row(&mut interleaved_segments, segment, row);
        }

        if let Some(segment) = current {
            anonymous_segments.push(segment);
        }

        if !saw_tagged_syntax {
            let interleaved_segments = anonymous_segments.clone();
            return Ok(Self {
                segments: anonymous_segments,
                interleaved_segments,
            });
        }

        let mut segments = Vec::new();
        for lane_key in lane_order {
            match lane_key {
                ToonlLaneOrder::Anonymous => segments.extend(anonymous_segments.clone()),
                ToonlLaneOrder::Tagged(tag) => {
                    let mut lane = tagged_lanes
                        .remove(&tag)
                        .expect("lane order only contains declared lanes");
                    segments.append(&mut lane.segments);
                    if let Some(segment) = lane.current {
                        segments.push(segment);
                    }
                }
            }
        }

        Ok(Self {
            segments,
            interleaved_segments,
        })
    }

    pub fn segments(&self) -> &[ToonlSegment] {
        &self.segments
    }

    pub fn row_values(&self) -> Result<Vec<Value>, ToonlError> {
        let mut values = Vec::new();
        for segment in &self.segments {
            for row in &segment.rows {
                values.push(segment.row_value(row, 0)?);
            }
        }
        Ok(values)
    }

    pub fn close_transform_documents(&self) -> Result<Vec<String>, ToonlError> {
        Ok(self
            .segments
            .iter()
            .map(ToonlSegment::to_closed_toon_document)
            .collect())
    }

    pub fn close_transform_interleaved_documents(&self) -> Result<Vec<String>, ToonlError> {
        Ok(self
            .interleaved_segments
            .iter()
            .map(ToonlSegment::to_closed_toon_document)
            .collect())
    }
}
