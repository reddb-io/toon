// Event-based streaming decoder (ADR 0006), targeting TOON spec v4.1.
//
// The Rust twin of `packages/toon/src/decode/stream.ts`: consumes a document,
// produces the six JSON-semantic events, each carrying its 1-based source
// line. Errors are fail-fast positioned `ParseError`s; strict-mode policy is
// resolved at this public boundary. The shared event-sequence fixtures under
// `tests/corpus/events/` are the parity contract between the two ports.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum ToonEvent {
    StartObject { line: usize },
    EndObject { line: usize },
    StartArray { length: usize, line: usize },
    EndArray { line: usize },
    Key { key: String, line: usize },
    Primitive { value: Value, line: usize },
}

#[derive(Debug, Clone)]
pub struct DecodeStreamOptions {
    pub indent: usize,
    pub strict: bool,
    /// Reconstruct cyclic discriminated arrays instead of returning metadata.
    pub cyclic_discriminated_arrays: bool,
    /// Decode primitive-array columns, recursive child tables, and matrices.
    pub object_array_columns: bool,
    /// Maximum nesting depth. `0` disables the guard for trusted input.
    pub max_depth: usize,
}

impl Default for DecodeStreamOptions {
    fn default() -> Self {
        Self {
            indent: DEFAULT_INDENT,
            strict: true,
            cyclic_discriminated_arrays: false,
            object_array_columns: true,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

struct StreamCtx {
    indent_size: usize,
    strict: bool,
    max_depth: usize,
}

#[derive(Debug, Clone)]
struct StreamLine {
    number: usize,
    depth: usize,
    content: String,
    /// A blank line appeared between the previous content line and this one.
    blank_before: bool,
}

fn stream_error(line: usize, message: &'static str) -> ParseError {
    ParseError { line, message, max_depth: None }
}

fn stream_depth_error(line: usize, max_depth: usize) -> ParseError {
    ParseError {
        line,
        message: "maximum nesting depth exceeded",
        max_depth: Some(max_depth),
    }
}

/// A full-line comment: only U+0020 spaces before `#` (§5.1).
fn is_comment_line(raw: &str) -> bool {
    raw.trim_start_matches(' ').starts_with('#')
}

/// Token trimming is exactly U+0020 (§12).
fn trim_u0020(text: &str) -> &str {
    text.trim_matches(' ')
}

fn classify_stream_lines(input: &str, ctx: &StreamCtx) -> Result<Vec<StreamLine>, ParseError> {
    let mut lines = Vec::new();
    let mut blank_pending = false;
    for (index, raw_line) in input.split('\n').enumerate() {
        let number = index + 1;
        let mut raw = raw_line;
        if number == 1 {
            // A single leading U+FEFF is a byte-order mark, not content (§12).
            raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
        }
        raw = raw.strip_suffix('\r').unwrap_or(raw);
        // Trailing spaces are not part of the line's content (§12).
        raw = raw.trim_end_matches(' ');
        if raw.trim().is_empty() {
            blank_pending = true;
            continue;
        }
        if is_comment_line(raw) {
            continue;
        }
        let mut i = 0;
        let mut tabs = 0usize;
        let mut spaces = 0usize;
        for ch in raw.chars() {
            match ch {
                ' ' => spaces += 1,
                '\t' => {
                    if ctx.strict {
                        return Err(stream_error(number, "tab used as indentation"));
                    }
                    tabs += 1;
                }
                _ => break,
            }
            i += ch.len_utf8();
        }
        let mut depth = if spaces % ctx.indent_size == 0 {
            spaces / ctx.indent_size
        } else if ctx.strict {
            return Err(stream_error(number, "invalid indentation"));
        } else {
            spaces / ctx.indent_size
        };
        // Non-strict tab leniency (§12): each leading tab counts as one level.
        depth += tabs;
        if ctx.max_depth != 0 && depth > ctx.max_depth {
            return Err(stream_depth_error(number, ctx.max_depth));
        }
        check_stream_header_depth(&raw[i..], number, ctx.max_depth)?;
        lines.push(StreamLine {
            number,
            depth,
            content: raw[i..].to_owned(),
            blank_before: blank_pending,
        });
        blank_pending = false;
    }
    Ok(lines)
}

fn check_stream_header_depth(
    content: &str,
    line: usize,
    max_depth: usize,
) -> Result<(), ParseError> {
    if max_depth == 0 {
        return Ok(());
    }
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    for character in content.chars() {
        if escaped {
            escaped = false;
        } else if quoted && character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if !quoted && character == '{' {
            depth += 1;
            if depth > max_depth {
                return Err(stream_depth_error(line, max_depth));
            }
        } else if !quoted && character == '}' {
            depth = depth.saturating_sub(1);
        }
    }
    Ok(())
}

// #region Header grammar (§6)

#[derive(Debug, Clone)]
struct StreamFieldNode {
    name: String,
    children: Option<Vec<StreamFieldNode>>,
}

#[derive(Debug, Clone)]
struct StreamHeader {
    key: Option<String>,
    length: usize,
    delimiter: char,
    fields: Option<Vec<StreamFieldNode>>,
    keyed: bool,
    inline: Option<String>,
}

fn valid_stream_length(segment: &str) -> bool {
    !segment.is_empty()
        && segment.chars().all(|c| c.is_ascii_digit())
        && (segment == "0" || !segment.starts_with('0'))
}

/// Parses one line's content as a header. `Ok(None)` when the content is not
/// header-shaped at all; `Err` on a malformed header — callers decide the
/// non-strict fall-through to key-value parsing (§6, §14.2).
fn parse_stream_header(content: &str, line: usize) -> Result<Option<StreamHeader>, ParseError> {
    let bracket = match find_unquoted(content, '[', line)? {
        Some(index) => index,
        None => return Ok(None),
    };
    if let Some(colon) = find_unquoted(content, ':', line)? {
        if colon < bracket {
            return Ok(None);
        }
    }

    let key_text = &content[..bracket];
    if !key_text.is_empty() && key_text.ends_with(|c: char| c.is_whitespace()) {
        return Err(stream_error(line, "whitespace between key and bracket segment"));
    }
    let close = match content[bracket..].find(']') {
        Some(offset) => bracket + offset,
        None => return Ok(None),
    };

    let mut segment = &content[bracket + 1..close];
    let mut keyed = false;
    let mut delimiter = ',';
    // A colon immediately after the length marks a keyed header (§9.5).
    if let Some(segment_colon) = segment.find(':') {
        let after = &segment[segment_colon + 1..];
        segment = &segment[..segment_colon];
        keyed = true;
        match after {
            "" => {}
            "\t" => delimiter = '\t',
            "|" => delimiter = '|',
            _ => return Err(stream_error(line, "malformed bracket segment")),
        }
    } else if segment.ends_with('\t') || segment.ends_with('|') {
        delimiter = segment.chars().last().unwrap();
        segment = &segment[..segment.len() - 1];
    }
    if !valid_stream_length(segment) {
        return Err(stream_error(line, "malformed array header length"));
    }
    let length: usize = segment
        .parse()
        .map_err(|_| stream_error(line, "malformed array header length"))?;

    let mut rest = &content[close + 1..];
    let mut fields = None;
    if rest.starts_with('{') {
        let end_brace = match_stream_brace(rest, line)?;
        fields = Some(parse_stream_field_list(&rest[1..end_brace], delimiter, line)?);
        rest = &rest[end_brace + 1..];
    }
    if keyed && fields.is_none() {
        return Err(stream_error(line, "keyed header requires a field list"));
    }
    if !rest.starts_with(':') {
        return Err(stream_error(line, "expected colon after array header"));
    }
    let inline_content = trim_u0020(&rest[1..]);
    if (fields.is_some() || keyed) && !inline_content.is_empty() {
        return Err(stream_error(line, "unexpected content after fields-bearing header colon"));
    }
    let key = if key_text.is_empty() {
        None
    } else {
        Some(parse_key(key_text, line)?.0)
    };
    Ok(Some(StreamHeader {
        key,
        length,
        delimiter,
        fields,
        keyed,
        inline: if inline_content.is_empty() { None } else { Some(inline_content.to_owned()) },
    }))
}

/// Byte index of the matching `}` for a field list starting at `{`, ignoring
/// quoted names.
fn match_stream_brace(text: &str, line: usize) -> Result<usize, ParseError> {
    let mut depth = 0i32;
    let mut in_quotes = false;
    let mut skip_next = false;
    for (i, ch) in text.char_indices() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if in_quotes {
            match ch {
                '\\' => skip_next = true,
                '"' => in_quotes = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_quotes = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
    }
    Err(stream_error(line, "malformed tabular header fields"))
}

fn parse_stream_field_list(
    text: &str,
    delimiter: char,
    line: usize,
) -> Result<Vec<StreamFieldNode>, ParseError> {
    let mut entries = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_quotes = false;
    let mut skip_next = false;
    for (i, ch) in text.char_indices() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if in_quotes {
            match ch {
                '\\' => skip_next = true,
                '"' => in_quotes = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_quotes = true,
            '{' => depth += 1,
            '}' => depth -= 1,
            c if c == delimiter && depth == 0 => {
                entries.push(parse_stream_field_entry(&text[start..i], delimiter, line)?);
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    entries.push(parse_stream_field_entry(&text[start..], delimiter, line)?);
    Ok(entries)
}

fn parse_stream_field_entry(
    chunk: &str,
    delimiter: char,
    line: usize,
) -> Result<StreamFieldNode, ParseError> {
    let trimmed = trim_u0020(chunk);
    if trimmed.is_empty() {
        return Err(stream_error(line, "empty field entry in header"));
    }
    let brace = if trimmed.starts_with('"') {
        let closing = stream_closing_quote(trimmed, line)?;
        trimmed[closing..].find('{').map(|offset| closing + offset)
    } else {
        trimmed.find('{')
    };
    let brace = match brace {
        Some(index) => index,
        None => {
            return Ok(StreamFieldNode { name: parse_key(trimmed, line)?.0, children: None });
        }
    };
    let end = match_stream_brace(&trimmed[brace..], line)? + brace;
    if end != trimmed.len() - 1 {
        return Err(stream_error(line, "malformed tabular header fields"));
    }
    let children = parse_stream_field_list(&trimmed[brace + 1..end], delimiter, line)?;
    if children.is_empty() {
        return Err(stream_error(line, "empty field entry in header"));
    }
    Ok(StreamFieldNode { name: parse_key(&trimmed[..brace], line)?.0, children: Some(children) })
}

/// Byte index just past the closing quote of a leading quoted token.
fn stream_closing_quote(text: &str, line: usize) -> Result<usize, ParseError> {
    let mut skip_next = false;
    for (i, ch) in text.char_indices().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        match ch {
            '\\' => skip_next = true,
            '"' => return Ok(i + ch.len_utf8()),
            _ => {}
        }
    }
    Err(stream_error(line, "invalid quoted string"))
}

fn count_stream_leaves(fields: &[StreamFieldNode]) -> usize {
    fields
        .iter()
        .map(|f| match &f.children {
            None => 1,
            Some(children) => count_stream_leaves(children),
        })
        .sum()
}

/// Duplicate field names within one field list are a header defect (§9.3, §14.2).
fn assert_no_duplicate_stream_fields(
    fields: &[StreamFieldNode],
    line: usize,
    ctx: &StreamCtx,
) -> Result<(), ParseError> {
    let mut seen = HashSet::new();
    for field in fields {
        if !seen.insert(field.name.clone()) && ctx.strict {
            return Err(stream_error(line, "duplicate field name in header"));
        }
        if let Some(children) = &field.children {
            assert_no_duplicate_stream_fields(children, line, ctx)?;
        }
    }
    Ok(())
}

// #endregion

/// Splits on the active delimiter, quote-aware, preserving empty tokens and
/// trimming exactly U+0020 around each token (§11.2). Content that trims to
/// nothing is zero cells.
fn split_stream_cells(content: &str, delimiter: char, _line: usize) -> Vec<String> {
    if trim_u0020(content).is_empty() {
        return Vec::new();
    }
    let mut cells = Vec::new();
    let mut start = 0usize;
    let mut in_quotes = false;
    let mut skip_next = false;
    for (i, ch) in content.char_indices() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if in_quotes {
            match ch {
                '\\' => skip_next = true,
                '"' => in_quotes = false,
                _ => {}
            }
            continue;
        }
        if ch == '"' {
            in_quotes = true;
        } else if ch == delimiter {
            cells.push(trim_u0020(&content[start..i]).to_owned());
            start = i + ch.len_utf8();
        }
    }
    cells.push(trim_u0020(&content[start..]).to_owned());
    cells
}

struct StreamReader {
    lines: Vec<StreamLine>,
    index: usize,
    /// Depth of open header spans — blank lines inside one are strict errors (§12).
    span_active: usize,
}

impl StreamReader {
    fn peek(&self) -> Option<&StreamLine> {
        self.lines.get(self.index)
    }
    fn take(&mut self, ctx: &StreamCtx) -> Result<StreamLine, ParseError> {
        let line = self.lines[self.index].clone();
        self.index += 1;
        if ctx.strict && self.span_active > 0 && line.blank_before {
            return Err(stream_error(line.number, "blank line inside a header span"));
        }
        Ok(line)
    }
    fn last_number(&self, fallback: usize) -> usize {
        if self.index == 0 { fallback } else { self.lines[self.index - 1].number }
    }
}

fn is_stream_key_value(content: &str, line: usize) -> Result<bool, ParseError> {
    Ok(find_unquoted(content, ':', line)?.is_some())
}

/// §7.4: decoders accept any token as a key; quoted keys unescape per §7.1.
fn decode_stream_key(token: &str, line: usize) -> Result<String, ParseError> {
    match parse_key(token, line) {
        Ok((key, _)) => Ok(key),
        Err(error) => {
            if token.starts_with('"') {
                Err(error)
            } else {
                Ok(token.to_owned())
            }
        }
    }
}

/// §14.3: duplicate sibling keys are a strict-mode error; non-strict is LWW.
fn record_stream_key(
    seen: &mut HashSet<String>,
    key: &str,
    line: usize,
    ctx: &StreamCtx,
) -> Result<(), ParseError> {
    if !seen.insert(key.to_owned()) && ctx.strict {
        return Err(stream_error(line, "duplicate object key"));
    }
    Ok(())
}

/// Decodes the document into the full event sequence, stopping at the first
/// error. The events emitted before the error are returned alongside it, so
/// iterator consumers observe the same prefix the TS generator yields.
pub fn decode_events(
    input: &str,
    options: &DecodeStreamOptions,
) -> (Vec<ToonEvent>, Option<ParseError>) {
    let ctx = StreamCtx {
        indent_size: options.indent.max(1),
        strict: options.strict,
        max_depth: options.max_depth,
    };
    let mut events = Vec::new();
    let error = decode_events_into(input, &ctx, &mut events).err();
    (events, error)
}

/// Iterator over positioned decode events; the streaming public surface.
pub struct EventDecoder {
    events: std::vec::IntoIter<ToonEvent>,
    error: Option<ParseError>,
}

impl Iterator for EventDecoder {
    type Item = Result<ToonEvent, ParseError>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.events.next() {
            Some(event) => Some(Ok(event)),
            None => self.error.take().map(Err),
        }
    }
}

pub fn decode_event_stream(input: &str, options: &DecodeStreamOptions) -> EventDecoder {
    let (events, error) = decode_events(input, options);
    EventDecoder { events: events.into_iter(), error }
}

fn decode_events_into(
    input: &str,
    ctx: &StreamCtx,
    out: &mut Vec<ToonEvent>,
) -> Result<(), ParseError> {
    let mut reader =
        StreamReader { lines: classify_stream_lines(input, ctx)?, index: 0, span_active: 0 };

    let first = match reader.peek() {
        None => {
            out.push(ToonEvent::StartObject { line: 1 });
            out.push(ToonEvent::EndObject { line: 1 });
            return Ok(());
        }
        Some(line) => line.clone(),
    };
    if first.depth != 0 {
        return Err(stream_error(first.number, "invalid indentation"));
    }

    // Root form discovery (§5).
    if first.content == "[]" {
        reader.take(ctx)?;
        out.push(ToonEvent::StartArray { length: 0, line: first.number });
        out.push(ToonEvent::EndArray { line: first.number });
        return expect_stream_end(&mut reader, ctx);
    }

    let mut header_failed = false;
    let header = match parse_stream_header(&first.content, first.number) {
        Ok(value) => value,
        Err(error) => {
            if ctx.strict {
                return Err(error);
            }
            header_failed = true;
            None
        }
    };

    if let Some(header) = header {
        if header.key.is_none() {
            reader.take(ctx)?;
            if header.keyed {
                emit_keyed_object(&mut reader, &first, &header, ctx, out)?;
            } else {
                emit_array(&mut reader, &first, &header, ctx, out)?;
            }
            return expect_stream_end(&mut reader, ctx);
        }
        // fall through to object parsing with this line as the first entry
    } else if reader.lines.len() == 1
        && !is_stream_key_value(&first.content, first.number)?
    {
        let _ = header_failed;
        reader.take(ctx)?;
        out.push(ToonEvent::Primitive {
            value: parse_scalar(trim_u0020(&first.content), first.number)?,
            line: first.number,
        });
        return Ok(());
    }

    emit_object(&mut reader, 0, first.number, ctx, out)?;
    expect_stream_end(&mut reader, ctx)
}

fn expect_stream_end(reader: &mut StreamReader, ctx: &StreamCtx) -> Result<(), ParseError> {
    if let Some(trailing) = reader.peek() {
        if ctx.strict {
            return Err(stream_error(trailing.number, "expected end of document"));
        }
    }
    reader.index = reader.lines.len();
    Ok(())
}

// #region Objects (§8)

fn emit_object(
    reader: &mut StreamReader,
    depth: usize,
    start_line: usize,
    ctx: &StreamCtx,
    out: &mut Vec<ToonEvent>,
) -> Result<(), ParseError> {
    out.push(ToonEvent::StartObject { line: start_line });
    let mut seen = HashSet::new();
    loop {
        let line = match reader.peek() {
            None => break,
            Some(line) => line.clone(),
        };
        if line.depth < depth {
            break;
        }
        if line.depth > depth {
            return Err(stream_error(line.number, "over-indented line"));
        }
        reader.take(ctx)?;
        let content = line.content.clone();
        emit_entry(reader, &line, &content, depth, ctx, &mut seen, out)?;
    }
    out.push(ToonEvent::EndObject { line: reader.last_number(start_line) });
    Ok(())
}

/// One object entry whose content sits at `depth` (possibly carried on a
/// hyphen line, §10).
#[allow(clippy::too_many_arguments)]
fn emit_entry(
    reader: &mut StreamReader,
    line: &StreamLine,
    content: &str,
    depth: usize,
    ctx: &StreamCtx,
    seen: &mut HashSet<String>,
    out: &mut Vec<ToonEvent>,
) -> Result<(), ParseError> {
    let header = match parse_stream_header(content, line.number) {
        Ok(value) => value,
        Err(error) => {
            if ctx.strict {
                return Err(error);
            }
            None
        }
    };

    if let Some(header) = header {
        match &header.key {
            None => {
                if ctx.strict {
                    return Err(stream_error(
                        line.number,
                        "keyless header is only valid at the root or as a list item",
                    ));
                }
                // non-strict: fall through to key-value parsing below
            }
            Some(key) => {
                record_stream_key(seen, key, line.number, ctx)?;
                out.push(ToonEvent::Key { key: key.clone(), line: line.number });
                let standing = StreamLine { depth, ..line.clone() };
                if header.keyed {
                    emit_keyed_object(reader, &standing, &header, ctx, out)?;
                } else {
                    emit_array(reader, &standing, &header, ctx, out)?;
                }
                return Ok(());
            }
        }
    }

    let colon = find_unquoted(content, ':', line.number)?
        .ok_or_else(|| stream_error(line.number, "expected key-value line"))?;
    let key = decode_stream_key(trim_u0020(&content[..colon]), line.number)?;
    let rest = trim_u0020(&content[colon + 1..]);
    record_stream_key(seen, &key, line.number, ctx)?;
    out.push(ToonEvent::Key { key, line: line.number });

    if rest.is_empty() {
        let child = reader.peek().cloned();
        if let Some(child) = child {
            if child.depth > depth {
                if child.depth != depth + 1 {
                    return Err(stream_error(child.number, "over-indented line"));
                }
                return emit_object(reader, depth + 1, child.number, ctx, out);
            }
        }
        out.push(ToonEvent::StartObject { line: line.number });
        out.push(ToonEvent::EndObject { line: line.number });
        return Ok(());
    }
    if rest == "[]" {
        out.push(ToonEvent::StartArray { length: 0, line: line.number });
        out.push(ToonEvent::EndArray { line: line.number });
        return Ok(());
    }
    out.push(ToonEvent::Primitive { value: parse_scalar(rest, line.number)?, line: line.number });
    Ok(())
}

// #endregion

// #region Arrays (§9.1, §9.2, §9.4) and list items (§10)

fn emit_array(
    reader: &mut StreamReader,
    header: &StreamLine,
    info: &StreamHeader,
    ctx: &StreamCtx,
    out: &mut Vec<ToonEvent>,
) -> Result<(), ParseError> {
    out.push(ToonEvent::StartArray { length: info.length, line: header.number });

    if let Some(fields) = &info.fields {
        assert_no_duplicate_stream_fields(fields, header.number, ctx)?;
        return emit_tabular_rows(reader, header, info, fields, ctx, out);
    }

    if let Some(inline) = &info.inline {
        let values = split_stream_cells(inline, info.delimiter, header.number);
        assert_stream_count(values.len(), info.length, header.number, ctx)?;
        for value in values {
            out.push(ToonEvent::Primitive {
                value: parse_scalar(&value, header.number)?,
                line: header.number,
            });
        }
        out.push(ToonEvent::EndArray { line: header.number });
        return Ok(());
    }

    // List form: items at depth +1, each `- …` or the bare `-` (§9.4, §10).
    let mut items = 0usize;
    loop {
        let line = match reader.peek() {
            None => break,
            Some(line) => line.clone(),
        };
        if line.depth <= header.depth {
            break;
        }
        if line.depth != header.depth + 1 {
            return Err(stream_error(line.number, "over-indented line"));
        }
        if !line.content.starts_with("- ") && line.content != "-" {
            break;
        }
        reader.take(ctx)?;
        if items == 0 {
            reader.span_active += 1;
        }
        items += 1;
        emit_list_item(reader, &line, ctx, out)?;
    }
    if items > 0 {
        reader.span_active -= 1;
    }

    let end_line = reader.last_number(header.number);
    if ctx.strict && items != info.length {
        return Err(stream_error(end_line, "array count mismatch"));
    }
    out.push(ToonEvent::EndArray { line: end_line });
    Ok(())
}

fn emit_list_item(
    reader: &mut StreamReader,
    line: &StreamLine,
    ctx: &StreamCtx,
    out: &mut Vec<ToonEvent>,
) -> Result<(), ParseError> {
    if line.content == "-" {
        out.push(ToonEvent::StartObject { line: line.number });
        out.push(ToonEvent::EndObject { line: line.number });
        return Ok(());
    }
    let trimmed = trim_u0020(&line.content[2..]).to_owned();

    if trimmed == "[]" {
        out.push(ToonEvent::StartArray { length: 0, line: line.number });
        out.push(ToonEvent::EndArray { line: line.number });
        return Ok(());
    }

    let header = match parse_stream_header(&trimmed, line.number) {
        Ok(value) => value,
        Err(error) => {
            if ctx.strict {
                return Err(error);
            }
            None
        }
    };

    if let Some(header) = &header {
        if header.key.is_none() {
            // A keyless non-keyed, non-fields header on a hyphen line is the
            // item itself; a fields-bearing or keyed one is only valid at the
            // root (§6, §10).
            if header.keyed || header.fields.is_some() {
                if ctx.strict {
                    return Err(stream_error(
                        line.number,
                        "keyless fields-bearing header is only valid at the root",
                    ));
                }
                out.push(ToonEvent::Primitive {
                    value: parse_scalar(&trimmed, line.number)?,
                    line: line.number,
                });
                return Ok(());
            }
            return emit_array(reader, line, header, ctx, out);
        }
    }

    let is_object_item =
        header.as_ref().map(|h| h.key.is_some()).unwrap_or(false)
            || is_stream_key_value(&trimmed, line.number)?;
    if is_object_item {
        // Object as list item: the first field stands at depth d+1 (§10).
        out.push(ToonEvent::StartObject { line: line.number });
        let mut seen = HashSet::new();
        emit_entry(reader, line, &trimmed, line.depth + 1, ctx, &mut seen, out)?;
        loop {
            let next = match reader.peek() {
                None => break,
                Some(next) => next.clone(),
            };
            if next.depth != line.depth + 1 {
                break;
            }
            if next.content.starts_with("- ") || next.content == "-" {
                break;
            }
            reader.take(ctx)?;
            let content = next.content.clone();
            emit_entry(reader, &next, &content, line.depth + 1, ctx, &mut seen, out)?;
        }
        out.push(ToonEvent::EndObject { line: reader.last_number(line.number) });
        return Ok(());
    }

    out.push(ToonEvent::Primitive {
        value: parse_scalar(&trimmed, line.number)?,
        line: line.number,
    });
    Ok(())
}

// #endregion

// #region Tabular rows (§9.3)

fn emit_tabular_rows(
    reader: &mut StreamReader,
    header: &StreamLine,
    info: &StreamHeader,
    fields: &[StreamFieldNode],
    ctx: &StreamCtx,
    out: &mut Vec<ToonEvent>,
) -> Result<(), ParseError> {
    let leaf_count = count_stream_leaves(fields);
    let row_depth = header.depth + 1;
    let mut rows = 0usize;
    loop {
        let line = match reader.peek() {
            None => break,
            Some(line) => line.clone(),
        };
        if line.depth <= header.depth {
            break;
        }
        if line.depth != row_depth {
            return Err(stream_error(line.number, "over-indented line"));
        }
        if !is_stream_row(&line.content, info.delimiter, line.number)? {
            break;
        }
        reader.take(ctx)?;
        if rows == 0 {
            reader.span_active += 1;
        }
        rows += 1;
        let cells = split_stream_cells(&line.content, info.delimiter, line.number);
        assert_stream_count(cells.len(), leaf_count, line.number, ctx)?;
        let mut cursor = 0usize;
        emit_row_object(fields, &cells, &mut cursor, line.number, out)?;
    }
    if rows > 0 {
        reader.span_active -= 1;
    }
    let end_line = reader.last_number(header.number);
    if ctx.strict && rows != info.length {
        return Err(stream_error(end_line, "array count mismatch"));
    }
    out.push(ToonEvent::EndArray { line: end_line });
    Ok(())
}

/// Row/key-value disambiguation at row depth (§9.3).
fn is_stream_row(content: &str, delimiter: char, line: usize) -> Result<bool, ParseError> {
    let colon = find_unquoted(content, ':', line)?;
    let colon = match colon {
        None => return Ok(true),
        Some(index) => index,
    };
    match find_unquoted(content, delimiter, line)? {
        None => Ok(false),
        Some(delim) => Ok(delim < colon),
    }
}

fn emit_row_object(
    fields: &[StreamFieldNode],
    cells: &[String],
    cursor: &mut usize,
    line: usize,
    out: &mut Vec<ToonEvent>,
) -> Result<(), ParseError> {
    out.push(ToonEvent::StartObject { line });
    for field in fields {
        out.push(ToonEvent::Key { key: field.name.clone(), line });
        match &field.children {
            None => {
                let empty = String::new();
                let cell = cells.get(*cursor).unwrap_or(&empty);
                *cursor += 1;
                out.push(ToonEvent::Primitive { value: parse_scalar(cell, line)?, line });
            }
            Some(children) => emit_row_object(children, cells, cursor, line, out)?,
        }
    }
    out.push(ToonEvent::EndObject { line });
    Ok(())
}

// #endregion

// #region Keyed tabular objects (§9.5)

fn emit_keyed_object(
    reader: &mut StreamReader,
    header: &StreamLine,
    info: &StreamHeader,
    ctx: &StreamCtx,
    out: &mut Vec<ToonEvent>,
) -> Result<(), ParseError> {
    let fields = info.fields.as_ref().expect("keyed header always carries fields");
    assert_no_duplicate_stream_fields(fields, header.number, ctx)?;
    let leaf_count = count_stream_leaves(fields);
    let entry_depth = header.depth + 1;
    out.push(ToonEvent::StartObject { line: header.number });
    let mut seen = HashSet::new();
    let mut rows = 0usize;
    loop {
        let line = match reader.peek() {
            None => break,
            Some(line) => line.clone(),
        };
        if line.depth <= header.depth {
            break;
        }
        if line.depth != entry_depth {
            return Err(stream_error(line.number, "over-indented line"));
        }
        let colon = match find_unquoted(&line.content, ':', line.number)? {
            None => {
                if ctx.strict {
                    return Err(stream_error(line.number, "expected a keyed entry row"));
                }
                reader.take(ctx)?;
                continue;
            }
            Some(index) => index,
        };
        reader.take(ctx)?;
        if rows == 0 {
            reader.span_active += 1;
        }
        rows += 1;
        let key = decode_stream_key(trim_u0020(&line.content[..colon]), line.number)?;
        record_stream_key(&mut seen, &key, line.number, ctx)?;
        out.push(ToonEvent::Key { key, line: line.number });
        let cells = split_stream_cells(&line.content[colon + 1..], info.delimiter, line.number);
        assert_stream_count(cells.len(), leaf_count, line.number, ctx)?;
        let mut cursor = 0usize;
        emit_row_object(fields, &cells, &mut cursor, line.number, out)?;
    }
    if rows > 0 {
        reader.span_active -= 1;
    }
    let end_line = reader.last_number(header.number);
    if ctx.strict && rows != info.length {
        return Err(stream_error(end_line, "array count mismatch"));
    }
    out.push(ToonEvent::EndObject { line: end_line });
    Ok(())
}

// #endregion

fn assert_stream_count(
    got: usize,
    expected: usize,
    line: usize,
    ctx: &StreamCtx,
) -> Result<(), ParseError> {
    if ctx.strict && got != expected {
        return Err(stream_error(line, "array count mismatch"));
    }
    Ok(())
}

// #region Value building

pub fn build_value_from_events(events: &[ToonEvent]) -> Value {
    enum Slot {
        Object(Vec<Field>),
        Array(Vec<Value>),
    }
    let mut stack: Vec<(Slot, Option<String>)> = Vec::new();
    let mut root: Option<Value> = None;

    fn attach(stack: &mut [(Slot, Option<String>)], root: &mut Option<Value>, value: Value) {
        match stack.last_mut() {
            None => *root = Some(value),
            Some((Slot::Array(items), _)) => items.push(value),
            Some((Slot::Object(fields), pending)) => {
                if let Some(key) = pending.take() {
                    // duplicate keys are last-write-wins (§14.3)
                    if let Some(existing) = fields.iter_mut().find(|f| f.key == key) {
                        existing.value = value;
                    } else {
                        fields.push(Field { key, value });
                    }
                }
            }
        }
    }

    for event in events {
        match event {
            ToonEvent::StartObject { .. } => stack.push((Slot::Object(Vec::new()), None)),
            ToonEvent::StartArray { .. } => stack.push((Slot::Array(Vec::new()), None)),
            ToonEvent::EndObject { .. } | ToonEvent::EndArray { .. } => {
                if let Some((slot, _)) = stack.pop() {
                    let value = match slot {
                        Slot::Object(fields) => Value::Object(Document { fields }),
                        Slot::Array(items) => Value::Array(Array::List(items)),
                    };
                    attach(&mut stack, &mut root, value);
                }
            }
            ToonEvent::Key { key, .. } => {
                if let Some((_, pending)) = stack.last_mut() {
                    *pending = Some(key.clone());
                }
            }
            ToonEvent::Primitive { value, .. } => attach(&mut stack, &mut root, value.clone()),
        }
    }
    root.unwrap_or(Value::Object(Document { fields: Vec::new() }))
}

// #endregion
