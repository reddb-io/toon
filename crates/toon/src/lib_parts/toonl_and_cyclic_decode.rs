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
        write_array(&mut output, None, &self.values(), 0, false, options)?;
        Ok(output)
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
            for (field, value) in self.fields.iter().zip(row) {
                insert_tabular_path(&mut document, &field.path, value.clone());
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

fn expand_cyclic_discriminated_arrays(document: Document) -> Result<Document, ParseError> {
    if document.fields.is_empty() {
        return Ok(document);
    }
    let mut expanded = Document::default();
    for field in &document.fields {
        let Some(value) = cyclic_array_from_tabular_object(&field.value, 1)? else {
            return Ok(document);
        };
        expanded.fields.push(Field {
            key: field.key.clone(),
            value,
        });
    }
    Ok(expanded)
}

fn cyclic_array_from_tabular_object(
    value: &Value,
    line: usize,
) -> Result<Option<Value>, ParseError> {
    let Value::Object(section) = value else {
        return Ok(None);
    };
    if !is_cyclic_section_like(section) {
        return Ok(None);
    }
    let Some(Value::String(order)) = section.get("order") else {
        return Err(cyclic_invalid(line));
    };
    let Some(Value::String(discriminator)) = section.get("discriminator") else {
        return Err(cyclic_invalid(line));
    };
    let Some(rows) = section.get("rows").and_then(value_to_usize) else {
        return Err(cyclic_invalid(line));
    };
    let order = parse_cyclic_order(order, rows, line)?;
    let common_rows = match section.get("common") {
        Some(Value::Array(array)) => array
            .values()
            .into_iter()
            .map(|value| match value {
                Value::Object(document) => Some(document),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| cyclic_invalid(line))?,
        Some(_) => return Err(cyclic_invalid(line)),
        None => (0..rows).map(|_| Document::default()).collect(),
    };
    if common_rows.len() != rows {
        return Err(cyclic_len_error(line));
    }

    let mut groups: HashMap<String, Vec<Document>> = HashMap::new();
    for field in &section.fields {
        if matches!(
            field.key.as_str(),
            "order" | "discriminator" | "rows" | "common"
        ) {
            continue;
        }
        let Value::Array(array) = &field.value else {
            return Err(cyclic_invalid(line));
        };
        let group_rows = array
            .values()
            .into_iter()
            .map(|value| match value {
                Value::Object(document) => Some(document),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| cyclic_invalid(line))?;
        groups.insert(field.key.clone(), group_rows);
    }
    if groups.is_empty() {
        return Err(cyclic_invalid(line));
    }

    let mut cursors: HashMap<&str, usize> = HashMap::new();
    let mut values = Vec::with_capacity(rows);
    for (position, label) in order.iter().enumerate() {
        let group = groups
            .get(label)
            .ok_or_else(|| cyclic_group_len_error(line))?;
        let cursor = cursors.entry(label.as_str()).or_insert(0);
        let payload = group
            .get(*cursor)
            .ok_or_else(|| cyclic_group_len_error(line))?;
        *cursor += 1;
        values.push(Value::Object(merge_cyclic_row(
            discriminator,
            label,
            common_rows.get(position),
            payload,
            line,
        )?));
    }
    for (label, group) in &groups {
        if cursors.get(label.as_str()).copied().unwrap_or(0) != group.len() {
            return Err(cyclic_group_len_error(line));
        }
    }
    Ok(Some(Value::Array(Array::List(values))))
}

fn is_cyclic_section_like(document: &Document) -> bool {
    ["order", "discriminator", "rows"]
        .iter()
        .any(|key| document.get(key).is_some())
}

fn parse_cyclic_order(encoded: &str, len: usize, line: usize) -> Result<Vec<String>, ParseError> {
    let Some(rest) = encoded.strip_prefix("cycle(") else {
        return Err(cyclic_invalid(line));
    };
    let Some((cycle, repeats)) = rest.split_once(")*") else {
        return Err(cyclic_invalid(line));
    };
    if cycle.is_empty() || repeats.contains("+tail(") {
        return Err(cyclic_invalid(line));
    }
    let cycle = cycle
        .split(',')
        .map(|label| percent_decode(label).map_err(|_| cyclic_invalid(line)))
        .collect::<Result<Vec<_>, _>>()?;
    if cycle.iter().any(String::is_empty) {
        return Err(cyclic_invalid(line));
    }
    let repeats = parse_cyclic_usize(repeats, line)?;
    let order_len = cycle
        .len()
        .checked_mul(repeats)
        .ok_or_else(|| cyclic_invalid(line))?;
    if order_len != len {
        return Err(cyclic_len_error(line));
    }
    let mut order = Vec::with_capacity(len);
    for index in 0..order_len {
        order.push(cycle[index % cycle.len()].clone());
    }
    Ok(order)
}

fn merge_cyclic_row(
    discriminator: &str,
    label: &str,
    common: Option<&Document>,
    payload: &Document,
    line: usize,
) -> Result<Document, ParseError> {
    let mut row = Document::default();
    row.fields.push(Field {
        key: discriminator.to_owned(),
        value: Value::String(label.to_owned()),
    });
    if let Some(common) = common {
        for field in &common.fields {
            if field.key == discriminator {
                return Err(cyclic_invalid(line));
            }
            row.fields.push(field.clone());
        }
    }
    let payload = inflate_cyclic_flat_document(payload, line)?;
    let Value::Object(payload) = payload else {
        return Err(cyclic_invalid(line));
    };
    for field in &payload.fields {
        if field.key == discriminator || row.fields.iter().any(|existing| existing.key == field.key)
        {
            return Err(cyclic_invalid(line));
        }
        row.fields.push(field.clone());
    }
    Ok(row)
}

fn parse_cyclic_usize(input: &str, line: usize) -> Result<usize, ParseError> {
    if input.is_empty()
        || (input.len() > 1 && input.starts_with('0'))
        || !input.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(cyclic_invalid(line));
    }
    input.parse().map_err(|_| cyclic_invalid(line))
}

fn value_to_usize(value: &Value) -> Option<usize> {
    let Value::Number(value) = value else {
        return None;
    };
    parse_cyclic_usize(value, 1).ok()
}

fn percent_decode(input: &str) -> Result<String, ()> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(());
            }
            let high = hex_value(bytes[index + 1]).ok_or(())?;
            let low = hex_value(bytes[index + 2]).ok_or(())?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| ())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn cyclic_invalid(line: usize) -> ParseError {
    ParseError {
        line,
        message: "invalid cyclic array wire",
        max_depth: None,
    }
}

fn cyclic_len_error(line: usize) -> ParseError {
    ParseError {
        line,
        message: "cyclic array length mismatch",
        max_depth: None,
    }
}

fn cyclic_group_len_error(line: usize) -> ParseError {
    ParseError {
        line,
        message: "cyclic array group length mismatch",
        max_depth: None,
    }
}

// ---------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------

