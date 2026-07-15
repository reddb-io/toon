//! TOON (Token-Oriented Object Notation) parser and serializer.
//!
//! Implements the v3.3 working draft hosted at <https://github.com/toon-format/spec>.
//! The decoder honours the spec's decoder options (`indent`, `strict`, `expandPaths`);
//! the encoder emits the canonical default profile: comma document delimiter,
//! two-space indentation, no key folding.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::{BufRead, Write};

/// Spaces per indentation level unless [`ParseOptions::indent`] says otherwise.
pub const DEFAULT_INDENT: usize = 2;

/// The document delimiter used by the encoder (spec §11.1, default profile).
const DOCUMENT_DELIMITER: char = ',';
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
    fields: Vec<Vec<String>>,
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
    fields: Option<Vec<Vec<String>>>,
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

fn append_interleaved_toonl_row(
    interleaved_segments: &mut Vec<ToonlSegment>,
    source: &ToonlSegment,
    row: Vec<String>,
) {
    if let Some(last) = interleaved_segments.last_mut() {
        if last.lane == source.lane
            && last.delimiter == source.delimiter
            && last.header_fields == source.header_fields
        {
            last.rows.push(row);
            return;
        }
    }
    interleaved_segments.push(ToonlSegment {
        lane: source.lane.clone(),
        delimiter: source.delimiter,
        fields: source.fields.clone(),
        header_fields: source.header_fields.clone(),
        rows: vec![row],
    });
}

impl ToonlSegment {
    pub fn delimiter(&self) -> char {
        self.delimiter
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }

    fn row_value(&self, row: &[String], line: usize) -> Result<Value, ToonlError> {
        toonl_row_value(&self.fields, row, line)
    }

    fn to_closed_toon_document(&self) -> String {
        let mut output = String::new();
        output.push('[');
        output.push_str(&self.rows.len().to_string());
        if self.delimiter != DOCUMENT_DELIMITER {
            output.push(self.delimiter);
        }
        output.push_str("]{");
        let fields = self
            .fields
            .iter()
            .map(|field| canonical_key(field))
            .collect::<Vec<_>>();
        output.push_str(&fields.join(&self.delimiter.to_string()));
        output.push_str("}:\n");
        for row in &self.rows {
            output.push_str("  ");
            output.push_str(&row.join(&self.delimiter.to_string()));
            output.push('\n');
        }
        output
    }
}

impl ToonlEncoder {
    pub fn new<T: AsRef<str>>(delimiter: char, fields: &[T]) -> Result<Self, ToonlError> {
        validate_toonl_delimiter(delimiter)?;
        let fields = normalize_toonl_header_fields(fields)?;
        let header_fields = toonl_header_fields(delimiter, &fields);
        let mut output = String::new();
        output.push_str(&toonl_header_text(delimiter, &header_fields, false));

        Ok(Self {
            delimiter,
            fields,
            header_fields,
            output,
            row_count: 0,
            rows_since_continuation: 0,
            bytes_since_continuation: 0,
            continuation_every_rows: None,
            continuation_every_bytes: None,
        })
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    pub fn set_continuation_every_rows(&mut self, rows: Option<usize>) -> Result<(), ToonlError> {
        validate_continuation_cadence(rows)?;
        self.continuation_every_rows = rows;
        Ok(())
    }

    pub fn set_continuation_every_bytes(&mut self, bytes: Option<usize>) -> Result<(), ToonlError> {
        validate_continuation_cadence(bytes)?;
        self.continuation_every_bytes = bytes;
        Ok(())
    }

    pub fn push_raw_row<T: AsRef<str>>(&mut self, cells: &[T]) -> Result<(), ToonlError> {
        if cells.len() != self.fields.len() {
            return Err(toonl_error(0, "row arity mismatch"));
        }
        let cells = cells
            .iter()
            .map(|cell| {
                let cell = cell.as_ref();
                parse_scalar(cell, 0).map_err(ToonlError::from_parse_error)?;
                Ok(cell.to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.write_continuation_if_due();
        let row = format!("{}\n", cells.join(&self.delimiter.to_string()));
        self.output.push_str(&row);
        self.row_count += 1;
        self.rows_since_continuation += 1;
        self.bytes_since_continuation += row.len();
        Ok(())
    }

    pub fn push_value_row(&mut self, value: &Value) -> Result<(), ToonlError> {
        let Value::Object(document) = value else {
            return Err(toonl_error(0, "TOONL output requires object rows"));
        };
        let mut cells = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            let Some(value) = document.get(field) else {
                return Err(toonl_error(0, "TOONL output schema changed"));
            };
            if !value.is_primitive() {
                return Err(toonl_error(0, "TOONL rows must be flat objects"));
            }
            cells.push(primitive_text(value, self.delimiter));
        }
        self.push_raw_row(&cells)
    }

    pub fn finish(mut self) -> String {
        self.output.push_str("[=");
        self.output.push_str(&self.row_count.to_string());
        self.output.push_str("]\n");
        self.output
    }

    fn write_continuation_if_due(&mut self) {
        if !continuation_due(
            self.continuation_every_rows,
            self.rows_since_continuation,
            self.continuation_every_bytes,
            self.bytes_since_continuation,
        ) {
            return;
        }
        self.output.push_str(&toonl_header_text(
            self.delimiter,
            &self.header_fields,
            true,
        ));
        self.rows_since_continuation = 0;
        self.bytes_since_continuation = 0;
    }
}

/// Encodes record values as TOONL.
///
/// Field order is canonicalized per record shape using the first order seen for
/// that shape. Later records with the same field set but a different call-site
/// order reuse the original order and do not force a header rotation.
pub fn encode_toonl_values(values: &[Value]) -> Result<String, ToonlError> {
    let mut output = String::new();
    let mut encoder: Option<ToonlEncoder> = None;
    let mut fields_by_shape = BTreeMap::new();

    for value in values {
        let fields = canonical_toonl_fields(toonl_value_fields(value)?, &mut fields_by_shape);
        if encoder
            .as_ref()
            .map_or(true, |encoder| encoder.fields() != fields.as_slice())
        {
            if let Some(encoder) = encoder.take() {
                output.push_str(&encoder.finish());
            }
            encoder = Some(ToonlEncoder::new(DOCUMENT_DELIMITER, &fields)?);
        }
        encoder
            .as_mut()
            .expect("encoder exists")
            .push_value_row(value)?;
    }

    if let Some(encoder) = encoder {
        output.push_str(&encoder.finish());
    }

    Ok(output)
}

impl<W: Write> ToonlWriter<W> {
    pub fn new(writer: W) -> Self {
        Self::with_delimiter(writer, DOCUMENT_DELIMITER)
    }

    pub fn with_delimiter(writer: W, delimiter: char) -> Self {
        Self {
            writer,
            delimiter,
            fields: None,
            header_fields: None,
            fields_by_shape: BTreeMap::new(),
            tagged_lanes: HashMap::new(),
            row_count: 0,
            rows_since_continuation: 0,
            bytes_since_continuation: 0,
            continuation_every_rows: None,
            continuation_every_bytes: None,
            finished: false,
        }
    }

    pub fn set_continuation_every_rows(&mut self, rows: Option<usize>) -> Result<(), ToonlError> {
        validate_continuation_cadence(rows)?;
        self.continuation_every_rows = rows;
        Ok(())
    }

    pub fn set_continuation_every_bytes(&mut self, bytes: Option<usize>) -> Result<(), ToonlError> {
        validate_continuation_cadence(bytes)?;
        self.continuation_every_bytes = bytes;
        Ok(())
    }

    pub fn write_record(&mut self, record: &Record) -> Result<(), ToonlError> {
        if self.finished {
            return Err(toonl_error(0, "TOONL writer is closed"));
        }
        validate_toonl_delimiter(self.delimiter)?;
        let fields = canonical_toonl_fields(toonl_value_fields(record)?, &mut self.fields_by_shape);

        if self.fields.as_ref() != Some(&fields) {
            self.close_segment()?;
            self.write_header(&fields)?;
            self.fields = Some(fields);
            self.row_count = 0;
            self.rows_since_continuation = 0;
            self.bytes_since_continuation = 0;
        }

        self.write_continuation_if_due()?;
        let bytes_written = self.write_value_row(record)?;
        self.row_count += 1;
        self.rows_since_continuation += 1;
        self.bytes_since_continuation += bytes_written;
        Ok(())
    }

    pub fn declare_lane<T: AsRef<str>>(
        &mut self,
        tag: &str,
        fields: &[T],
    ) -> Result<(), ToonlError> {
        if self.finished {
            return Err(toonl_error(0, "TOONL writer is closed"));
        }
        validate_toonl_tag(tag, 0)?;
        if !self.tagged_lanes.contains_key(tag)
            && self.tagged_lanes.len() >= TOONL_TAGGED_LANE_LIMIT
        {
            return Err(toonl_error(0, "too many tagged lanes"));
        }

        let fields = normalize_toonl_header_fields(fields)?;
        let header_fields = toonl_header_fields(DOCUMENT_DELIMITER, &fields);
        if self
            .tagged_lanes
            .get(tag)
            .and_then(|lane| lane.fields.as_ref())
            == Some(&fields)
        {
            return Ok(());
        }

        self.writer
            .write_all(tagged_toonl_header_text(tag, &header_fields).as_bytes())
            .map_err(write_toonl_error)?;
        self.tagged_lanes.entry(tag.to_owned()).or_default().fields = Some(fields);
        Ok(())
    }

    pub fn write_tagged_record(&mut self, tag: &str, record: &Record) -> Result<(), ToonlError> {
        if self.finished {
            return Err(toonl_error(0, "TOONL writer is closed"));
        }
        validate_toonl_tag(tag, 0)?;
        if !self.tagged_lanes.contains_key(tag)
            && self.tagged_lanes.len() >= TOONL_TAGGED_LANE_LIMIT
        {
            return Err(toonl_error(0, "too many tagged lanes"));
        }

        let fields = {
            let lane = self.tagged_lanes.entry(tag.to_owned()).or_default();
            canonical_toonl_fields(toonl_value_fields(record)?, &mut lane.fields_by_shape)
        };
        let header_fields = toonl_header_fields(DOCUMENT_DELIMITER, &fields);
        let cells = toonl_value_cells(record, &fields, DOCUMENT_DELIMITER)?;
        let needs_declaration = self
            .tagged_lanes
            .get(tag)
            .and_then(|lane| lane.fields.as_ref())
            != Some(&fields);

        if needs_declaration {
            self.writer
                .write_all(tagged_toonl_header_text(tag, &header_fields).as_bytes())
                .map_err(write_toonl_error)?;
            self.tagged_lanes
                .get_mut(tag)
                .expect("tagged lane exists")
                .fields = Some(fields);
        }

        let row = format!("{}:{}\n", tag, cells.join(&DOCUMENT_DELIMITER.to_string()));
        self.writer
            .write_all(row.as_bytes())
            .map_err(write_toonl_error)?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<W, ToonlError> {
        if !self.finished {
            self.close_segment()?;
            self.finished = true;
        }
        self.writer.flush().map_err(write_toonl_error)?;
        Ok(self.writer)
    }

    fn close_segment(&mut self) -> Result<(), ToonlError> {
        if self.fields.is_none() {
            return Ok(());
        }
        writeln!(self.writer, "[={}]", self.row_count).map_err(write_toonl_error)
    }

    fn write_header(&mut self, fields: &[String]) -> Result<(), ToonlError> {
        let header_fields = toonl_header_fields(self.delimiter, fields);
        self.writer
            .write_all(toonl_header_text(self.delimiter, &header_fields, false).as_bytes())
            .map_err(write_toonl_error)?;
        self.header_fields = Some(header_fields);
        Ok(())
    }

    fn write_continuation_if_due(&mut self) -> Result<(), ToonlError> {
        if !continuation_due(
            self.continuation_every_rows,
            self.rows_since_continuation,
            self.continuation_every_bytes,
            self.bytes_since_continuation,
        ) {
            return Ok(());
        }
        let header_fields = self
            .header_fields
            .as_ref()
            .expect("header fields are set before rows are written");
        self.writer
            .write_all(toonl_header_text(self.delimiter, header_fields, true).as_bytes())
            .map_err(write_toonl_error)?;
        self.rows_since_continuation = 0;
        self.bytes_since_continuation = 0;
        Ok(())
    }

    fn write_value_row(&mut self, value: &Record) -> Result<usize, ToonlError> {
        let fields = self
            .fields
            .as_ref()
            .expect("fields are set before rows are written");
        let cells = toonl_value_cells(value, fields, self.delimiter)?;
        let row = format!("{}\n", cells.join(&self.delimiter.to_string()));
        self.writer
            .write_all(row.as_bytes())
            .map_err(write_toonl_error)?;
        Ok(row.len())
    }
}

pub fn jsonl_to_toonl<R: BufRead, W: Write>(mut reader: R, writer: W) -> Result<(), ToonlError> {
    let mut line = String::new();
    let mut line_number = 0;
    let mut toonl = ToonlWriter::new(writer);

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => return Err(read_toonl_error(error)),
        }
        line_number += 1;
        let line = line.trim_end_matches('\n').trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let value = serde_json::from_str(line)
            .map(Value::from_json_value)
            .map_err(|error| toonl_error(line_number, format!("invalid JSONL: {error}")))?;
        toonl.write_record(&value)?;
    }

    toonl.finish().map(|_| ())
}

pub fn toonl_to_jsonl<R: BufRead, W: Write>(reader: R, mut writer: W) -> Result<(), ToonlError> {
    for record in ToonlReader::new(reader) {
        let record = record?;
        serde_json::to_writer(&mut writer, &record.to_json_value())
            .map_err(|error| toonl_error(0, format!("write error: {error}")))?;
        writer.write_all(b"\n").map_err(write_toonl_error)?;
    }
    writer.flush().map_err(write_toonl_error)
}

pub fn close_transform_stream<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
) -> Result<(), ToonlError> {
    let mut input = String::new();
    reader
        .read_to_string(&mut input)
        .map_err(read_toonl_error)?;
    for segment in ToonlStream::parse(&input)?.segments() {
        writer
            .write_all(segment.to_closed_toon_document().as_bytes())
            .map_err(write_toonl_error)?;
    }
    writer.flush().map_err(write_toonl_error)
}

pub fn close_transform_stream_interleaved<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
) -> Result<(), ToonlError> {
    let mut input = String::new();
    reader
        .read_to_string(&mut input)
        .map_err(read_toonl_error)?;
    for segment in ToonlStream::parse(&input)?.close_transform_interleaved_documents()? {
        writer
            .write_all(segment.as_bytes())
            .map_err(write_toonl_error)?;
    }
    writer.flush().map_err(write_toonl_error)
}

impl<R: BufRead> ToonlRowReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line: String::new(),
            line_number: 0,
            byte_offset: 0,
            active_header_line: None,
            rows_since_header: 0,
            anchor: None,
            current: None,
            tagged_lanes: HashMap::new(),
            finished: false,
        }
    }

    pub fn cursor(&self) -> Option<ToonlCursor> {
        self.active_header_line
            .as_ref()
            .map(|active_header_line| ToonlCursor {
                byte_offset: self.byte_offset,
                active_header_line: active_header_line.clone(),
                rows_since_header: self.rows_since_header,
                anchor: self.anchor.clone(),
            })
    }
}

impl<R: BufRead> Iterator for ToonlRowReader<R> {
    type Item = Result<Value, ToonlError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        loop {
            self.line.clear();
            let line_start_offset = self.byte_offset;
            match self.reader.read_line(&mut self.line) {
                Ok(0) => {
                    self.finished = true;
                    return None;
                }
                Ok(bytes_read) => {
                    self.byte_offset += bytes_read as u64;
                    self.anchor = Some(ToonlCursorAnchor {
                        byte_offset: line_start_offset,
                        bytes: self.line.clone(),
                    });
                }
                Err(error) => {
                    self.finished = true;
                    return Some(Err(toonl_error(0, format!("read error: {error}"))));
                }
            }
            self.line_number += 1;

            let line = self
                .line
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_owned();
            if line.is_empty() {
                continue;
            }
            if let Err(error) = self.consume_non_blank_line(&line) {
                self.finished = true;
                return Some(Err(error));
            }
            if let Some(row) = self.current_row_value(&line) {
                return Some(row);
            }
        }
    }
}

impl<R: BufRead> ToonlRowReader<R> {
    fn consume_non_blank_line(&mut self, line: &str) -> Result<(), ToonlError> {
        if line.starts_with("- ") {
            return Err(toonl_error(self.line_number, "reserved line prefix"));
        }
        if let Some(expected) = toonl_trailer_count(line, self.line_number)? {
            let segment = self
                .current
                .take()
                .ok_or_else(|| toonl_error(self.line_number, "trailer without header"))?;
            if segment.row_count != expected {
                return Err(toonl_error(self.line_number, "trailer count mismatch"));
            }
            return Ok(());
        }
        if let Some(header) = parse_toonl_header(line, self.line_number)? {
            if header.continuation {
                ensure_open_continuation_matches(self.current.as_ref(), &header, self.line_number)?;
                return Ok(());
            }
            if let Some(tag) = header.tag {
                if !self.tagged_lanes.contains_key(&tag)
                    && self.tagged_lanes.len() >= TOONL_TAGGED_LANE_LIMIT
                {
                    return Err(toonl_error(self.line_number, "too many tagged lanes"));
                }
                self.tagged_lanes.insert(
                    tag,
                    OpenToonlSegment {
                        delimiter: header.delimiter,
                        fields: header.fields,
                        header_fields: header.header_fields,
                        row_count: 0,
                    },
                );
                return Ok(());
            }
            self.active_header_line = Some(toonl_header_text(
                header.delimiter,
                &header.header_fields,
                false,
            ));
            self.rows_since_header = 0;
            self.current = Some(OpenToonlSegment {
                delimiter: header.delimiter,
                fields: header.fields,
                header_fields: header.header_fields,
                row_count: 0,
            });
        }
        Ok(())
    }

    fn current_row_value(&mut self, line: &str) -> Option<Result<Value, ToonlError>> {
        if line.starts_with('[')
            && (toonl_trailer_count(line, self.line_number)
                .ok()
                .flatten()
                .is_some()
                || parse_toonl_header(line, self.line_number)
                    .ok()
                    .flatten()
                    .is_some())
        {
            return None;
        }

        if let Some((tag, row_text)) = match toonl_tagged_row_prefix(line, self.line_number) {
            Ok(prefix) => prefix,
            Err(error) => return Some(Err(error)),
        } {
            if let Some(segment) = self.tagged_lanes.get_mut(tag) {
                let row = match parse_toonl_row(
                    row_text,
                    segment.delimiter,
                    segment.fields.len(),
                    self.line_number,
                ) {
                    Ok(row) => row,
                    Err(error) => return Some(Err(error)),
                };
                segment.row_count += 1;
                return Some(toonl_row_value(&segment.fields, &row, self.line_number));
            }
            if self.current.is_none() {
                return Some(Err(toonl_error(self.line_number, "unknown tag")));
            }
        }

        let Some(segment) = self.current.as_mut() else {
            return Some(Err(toonl_error(self.line_number, "row before header")));
        };
        let row = match parse_toonl_row(
            line,
            segment.delimiter,
            segment.fields.len(),
            self.line_number,
        ) {
            Ok(row) => row,
            Err(error) => return Some(Err(error)),
        };
        segment.row_count += 1;
        self.rows_since_header += 1;
        Some(toonl_row_value(&segment.fields, &row, self.line_number))
    }
}

impl ToonlRowReader<std::io::Cursor<Vec<u8>>> {
    pub fn resume_from_bytes(input: &[u8], cursor: ToonlCursor) -> Result<Self, ToonlResumeError> {
        if input.len() < cursor.byte_offset as usize {
            return Err(ToonlResumeError::Invalid(
                ToonlCursorInvalidation::Truncated {
                    byte_offset: cursor.byte_offset,
                    file_size: input.len() as u64,
                },
            ));
        }
        if let Some(anchor) = &cursor.anchor {
            let start = anchor.byte_offset as usize;
            let end = start.saturating_add(anchor.bytes.len());
            if input.get(start..end) != Some(anchor.bytes.as_bytes()) {
                return Err(ToonlResumeError::Invalid(
                    ToonlCursorInvalidation::AnchorMismatch {
                        byte_offset: anchor.byte_offset,
                    },
                ));
            }
        }

        let header_line = cursor
            .active_header_line
            .trim_end_matches('\n')
            .trim_end_matches('\r');
        let header = parse_toonl_header(header_line, 0)
            .map_err(ToonlResumeError::Parse)?
            .ok_or_else(|| {
                ToonlResumeError::Parse(toonl_error(0, "invalid cursor activeHeaderLine"))
            })?;
        if header.continuation || header.tag.is_some() {
            return Err(ToonlResumeError::Parse(toonl_error(
                0,
                "invalid cursor activeHeaderLine",
            )));
        }

        let suffix = input[cursor.byte_offset as usize..].to_vec();
        Ok(Self {
            reader: std::io::Cursor::new(suffix),
            line: String::new(),
            line_number: 0,
            byte_offset: cursor.byte_offset,
            active_header_line: Some(cursor.active_header_line),
            rows_since_header: cursor.rows_since_header,
            anchor: cursor.anchor,
            current: Some(OpenToonlSegment {
                delimiter: header.delimiter,
                fields: header.fields,
                header_fields: header.header_fields,
                row_count: cursor.rows_since_header,
            }),
            tagged_lanes: HashMap::new(),
            finished: false,
        })
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
            let mut document = Document::default();
            for (path, value) in self.fields.iter().zip(row) {
                insert_tabular_path(&mut document, path, value.clone());
            }
            Value::Object(document)
        })
    }
}

fn insert_tabular_path(document: &mut Document, path: &[String], value: Value) {
    let key = &path[0];
    if path.len() == 1 {
        document.fields.push(Field {
            key: key.clone(),
            value,
        });
        return;
    }

    let position = document
        .fields
        .iter()
        .position(|field| field.key == *key)
        .unwrap_or_else(|| {
            document.fields.push(Field {
                key: key.clone(),
                value: Value::Object(Document::default()),
            });
            document.fields.len() - 1
        });
    let Value::Object(nested) = &mut document.fields[position].value else {
        unreachable!("nested tabular header paths are validated before rows are decoded");
    };
    insert_tabular_path(nested, &path[1..], value);
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
    fields: &[Vec<String>],
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
        Some(parse_header_fields(
            &suffix[1..suffix.len() - 1],
            delimiter,
        )?)
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

fn parse_header_fields(source: &str, delimiter: char) -> Result<Vec<Vec<String>>, HeaderError> {
    struct Parser<'a> {
        source: &'a str,
        delimiter: u8,
        index: usize,
        paths: Vec<Vec<String>>,
    }

    impl Parser<'_> {
        fn parse_list(&mut self, prefix: &[String], nested: bool) -> Result<(), HeaderError> {
            let mut count = 0usize;
            while self.index < self.source.len() {
                let current = self.source.as_bytes()[self.index];
                if nested && current == b'}' {
                    break;
                }
                if current == self.delimiter || current == b'}' {
                    return Err(HeaderError("invalid array header"));
                }

                let start = self.index;
                while self.index < self.source.len() {
                    let character = self.source.as_bytes()[self.index];
                    if character == b'"' {
                        self.skip_quoted_header_key();
                        continue;
                    }
                    if character == self.delimiter || character == b'{' || character == b'}' {
                        break;
                    }
                    self.index += 1;
                }

                let (key, _) = parse_key(&self.source[start..self.index], 0)
                    .map_err(|_| HeaderError("invalid array header"))?;
                if key.is_empty() {
                    return Err(HeaderError("invalid array header"));
                }

                count += 1;
                if self.index < self.source.len() && self.source.as_bytes()[self.index] == b'{' {
                    self.index += 1;
                    let before = self.paths.len();
                    let mut nested_prefix = prefix.to_vec();
                    nested_prefix.push(key);
                    self.parse_list(&nested_prefix, true)?;
                    if self.index >= self.source.len()
                        || self.source.as_bytes()[self.index] != b'}'
                        || self.paths.len() == before
                    {
                        return Err(HeaderError("invalid array header"));
                    }
                    self.index += 1;
                } else {
                    let mut path = prefix.to_vec();
                    path.push(key);
                    self.paths.push(path);
                }

                if self.index < self.source.len()
                    && self.source.as_bytes()[self.index] == self.delimiter
                {
                    self.index += 1;
                    if self.index >= self.source.len() || self.source.as_bytes()[self.index] == b'}'
                    {
                        return Err(HeaderError("invalid array header"));
                    }
                    continue;
                }
                if (nested
                    && self.index < self.source.len()
                    && self.source.as_bytes()[self.index] == b'}')
                    || (!nested && self.index == self.source.len())
                {
                    break;
                }
                return Err(HeaderError("invalid array header"));
            }

            if count == 0 {
                return Err(HeaderError("invalid array header"));
            }
            Ok(())
        }

        fn skip_quoted_header_key(&mut self) {
            self.index += 1;
            while self.index < self.source.len() {
                let character = self.source.as_bytes()[self.index];
                self.index += 1;
                if character == b'\\' {
                    self.index += 1;
                } else if character == b'"' {
                    return;
                }
            }
        }
    }

    let mut parser = Parser {
        source,
        delimiter: delimiter as u8,
        index: 0,
        paths: Vec::new(),
    };
    parser.parse_list(&[], false)?;
    if parser.index != source.len() {
        return Err(HeaderError("invalid array header"));
    }
    for index in 0..parser.paths.len() {
        for other in index + 1..parser.paths.len() {
            let left = &parser.paths[index];
            let right = &parser.paths[other];
            if left == right || left.starts_with(right) || right.starts_with(left) {
                return Err(HeaderError("duplicate key"));
            }
        }
    }
    Ok(parser.paths)
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
    fn parses_nested_tabular_headers() {
        let document = Document::parse(
            "orders[2]{id,customer{name,country},total}:\n  1,Ada,UK,10.5\n  2,Bob,US,20\n",
        )
        .unwrap();

        assert_eq!(
            document.to_json_value(),
            serde_json::json!({
                "orders": [
                    { "id": 1, "customer": { "name": "Ada", "country": "UK" }, "total": 10.5 },
                    { "id": 2, "customer": { "name": "Bob", "country": "US" }, "total": 20 }
                ]
            })
        );
    }

    #[test]
    fn nested_tabular_headers_validate_leaf_arity_and_shape() {
        let arity = Document::parse("orders[1]{id,customer{name,country}}:\n  1,Ada\n")
            .expect_err("leaf count controls row arity");
        assert_eq!(arity.line(), 2);
        assert_eq!(arity.message(), "array row length mismatch");

        let empty = Document::parse("orders[1]{id,customer{}}:\n  1\n")
            .expect_err("empty nested groups are invalid");
        assert_eq!(empty.line(), 1);
        assert_eq!(empty.message(), "invalid array header");

        let duplicate = Document::parse("orders[1]{customer{name},customer{name}}:\n  Ada,Bob\n")
            .expect_err("duplicate leaf paths are invalid");
        assert_eq!(duplicate.line(), 1);
        assert_eq!(duplicate.message(), "duplicate key");

        let unbalanced = Document::parse("orders[1]{id,customer{name,country}:\n  1,Ada,UK\n")
            .expect_err("unbalanced nested groups are invalid");
        assert_eq!(unbalanced.line(), 1);
        assert_eq!(unbalanced.message(), "invalid array header");
    }

    #[test]
    fn treats_leading_plus_tokens_as_strings() {
        // The spec is silent on leading-plus tokens (upstream spec PR #52);
        // the reference implementation keeps them as strings while exponent
        // plus signs stay numeric.
        let document = Document::parse("values[3]: +1,+1.5,+1e2\nexponent: 1e+2\n").unwrap();

        assert_eq!(
            document.to_json_value(),
            serde_json::json!({"values": ["+1", "+1.5", "+1e2"], "exponent": 100})
        );
    }

    #[test]
    fn nested_empty_object_list_items_round_trip_as_bare_hyphen() {
        // The bare `-` marker for an empty object list item applies
        // recursively inside nested expanded arrays, with no trailing space
        // (upstream spec PR #53).
        let input = "items[2]:\n  - [1]:\n    -\n  - [2]:\n    - x\n    -\n";
        let document = Document::parse(input).unwrap();

        assert_eq!(
            document.to_json_value(),
            serde_json::json!({"items": [[{}], ["x", {}]]})
        );
        assert_eq!(document.to_canonical_toon(), input);
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
