// Event-based streaming decoder (ADR 0006), targeting TOON spec v4.1.
//
// The Rust twin of `packages/toon/src/decode/stream.ts`: consumes a document,
// produces the six JSON-semantic events, each carrying its 1-based source
// line. Errors are fail-fast positioned `ParseError`s; strict-mode policy is
// resolved at this public boundary. The shared event-sequence fixtures under
// `tests/corpus/events/` are the parity contract between the two ports.

use std::collections::HashSet;
use std::io::Cursor;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread::JoinHandle;

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

struct StreamReader<R> {
    input: R,
    lookahead: Option<StreamLine>,
    next_number: usize,
    last_number: Option<usize>,
    blank_pending: bool,
    at_eof: bool,
    /// Depth of open header spans — blank lines inside one are strict errors (§12).
    span_active: usize,
}

impl<R: BufRead> StreamReader<R> {
    fn new(input: R) -> Self {
        Self {
            input,
            lookahead: None,
            next_number: 1,
            last_number: None,
            blank_pending: false,
            at_eof: false,
            span_active: 0,
        }
    }

    fn fill(&mut self, ctx: &StreamCtx) -> Result<(), ParseError> {
        while self.lookahead.is_none() && !self.at_eof {
            let number = self.next_number;
            let mut raw = String::new();
            let read = self
                .input
                .read_line(&mut raw)
                .map_err(|_| stream_error(number, "failed to read input"))?;
            if read == 0 {
                self.at_eof = true;
                break;
            }
            self.next_number += 1;
            if raw.ends_with('\n') {
                raw.pop();
            }
            let mut text = raw.as_str();
            if number == 1 {
                text = text.strip_prefix('\u{feff}').unwrap_or(text);
            }
            text = text.strip_suffix('\r').unwrap_or(text);
            text = text.trim_end_matches(' ');
            if text.trim().is_empty() {
                self.blank_pending = true;
                continue;
            }
            if is_comment_line(text) {
                continue;
            }
            let mut offset = 0;
            let mut tabs = 0usize;
            let mut spaces = 0usize;
            for character in text.chars() {
                match character {
                    ' ' => spaces += 1,
                    '\t' => {
                        if ctx.strict {
                            return Err(stream_error(number, "tab used as indentation"));
                        }
                        tabs += 1;
                    }
                    _ => break,
                }
                offset += character.len_utf8();
            }
            let mut depth = if spaces % ctx.indent_size == 0 {
                spaces / ctx.indent_size
            } else if ctx.strict {
                return Err(stream_error(number, "invalid indentation"));
            } else {
                spaces / ctx.indent_size
            };
            depth += tabs;
            if ctx.max_depth != 0 && depth > ctx.max_depth {
                return Err(stream_depth_error(number, ctx.max_depth));
            }
            check_stream_header_depth(&text[offset..], number, ctx.max_depth)?;
            self.lookahead = Some(StreamLine {
                number,
                depth,
                content: text[offset..].to_owned(),
                blank_before: self.blank_pending,
            });
            self.blank_pending = false;
        }
        Ok(())
    }

    fn peek(&mut self, ctx: &StreamCtx) -> Result<Option<StreamLine>, ParseError> {
        self.fill(ctx)?;
        Ok(self.lookahead.clone())
    }

    fn take(&mut self, ctx: &StreamCtx) -> Result<StreamLine, ParseError> {
        self.fill(ctx)?;
        let line = self
            .lookahead
            .take()
            .expect("take is only called after successful lookahead");
        self.last_number = Some(line.number);
        if ctx.strict && self.span_active > 0 && line.blank_before {
            return Err(stream_error(line.number, "blank line inside a header span"));
        }
        Ok(line)
    }
    fn last_number(&self, fallback: usize) -> usize {
        self.last_number.unwrap_or(fallback)
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
    let error = decode_events_into(Cursor::new(input.as_bytes()), &ctx, &mut events).err();
    (events, error)
}

trait EventSink {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError>;
}

impl EventSink for Vec<ToonEvent> {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError> {
        self.push(event);
        Ok(())
    }
}

struct ChannelSink {
    sender: SyncSender<Result<ToonEvent, ParseError>>,
}

impl EventSink for ChannelSink {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError> {
        let line = event.line();
        self.sender
            .send(Ok(event))
            .map_err(|_| stream_error(line, "event consumer disconnected"))
    }
}

impl ToonEvent {
    fn line(&self) -> usize {
        match self {
            Self::StartObject { line }
            | Self::EndObject { line }
            | Self::StartArray { line, .. }
            | Self::EndArray { line }
            | Self::Key { line, .. }
            | Self::Primitive { line, .. } => *line,
        }
    }
}

/// Iterator over positioned decode events. A zero-capacity channel keeps the
/// parser coupled to iteration, so neither input nor events are accumulated.
pub struct EventDecoder {
    receiver: Receiver<Result<ToonEvent, ParseError>>,
    worker: Option<JoinHandle<()>>,
}

impl Iterator for EventDecoder {
    type Item = Result<ToonEvent, ParseError>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.receiver.recv() {
            Ok(event) => Some(event),
            Err(_) => {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                None
            }
        }
    }
}

impl Drop for EventDecoder {
    fn drop(&mut self) {
        // Replacing the receiver disconnects a parser blocked on event delivery.
        let (_sender, replacement) = sync_channel(0);
        let old = std::mem::replace(&mut self.receiver, replacement);
        drop(old);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Decode events directly from a buffered reader with one classified line of
/// lookahead. The reader is moved to a worker so each iterator step can suspend
/// the recursive grammar exactly at an event boundary.
pub fn decode_event_reader<R>(reader: R, options: &DecodeStreamOptions) -> EventDecoder
where
    R: BufRead + Send + 'static,
{
    let (sender, receiver) = sync_channel(0);
    let ctx = StreamCtx {
        indent_size: options.indent.max(1),
        strict: options.strict,
        max_depth: options.max_depth,
    };
    let error_sender = sender.clone();
    let worker = std::thread::spawn(move || {
        let mut sink = ChannelSink { sender };
        if let Err(error) = decode_events_into(reader, &ctx, &mut sink) {
            let _ = error_sender.send(Err(error));
        }
    });
    EventDecoder { receiver, worker: Some(worker) }
}

pub fn decode_event_stream(input: &str, options: &DecodeStreamOptions) -> EventDecoder {
    decode_event_reader(Cursor::new(input.as_bytes().to_vec()), options)
}

fn decode_events_into<R: BufRead, S: EventSink>(
    input: R,
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<(), ParseError> {
    let mut reader = StreamReader::new(input);

    let first = match reader.peek(ctx)? {
        None => {
            out.emit(ToonEvent::StartObject { line: 1 })?;
            out.emit(ToonEvent::EndObject { line: 1 })?;
            return Ok(());
        }
        Some(line) => line,
    };
    if first.depth != 0 {
        return Err(stream_error(first.number, "invalid indentation"));
    }

    // Root form discovery (§5).
    if first.content == "[]" {
        reader.take(ctx)?;
        out.emit(ToonEvent::StartArray { length: 0, line: first.number })?;
        out.emit(ToonEvent::EndArray { line: first.number })?;
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
    } else if !is_stream_key_value(&first.content, first.number)? {
        let _ = header_failed;
        reader.take(ctx)?;
        if reader.peek(ctx)?.is_none() {
            out.emit(ToonEvent::Primitive {
                value: parse_scalar(trim_u0020(&first.content), first.number)?,
                line: first.number,
            })?;
            return Ok(());
        }
        out.emit(ToonEvent::StartObject { line: first.number })?;
        let mut seen = HashSet::new();
        return emit_entry(
            &mut reader,
            &first,
            &first.content,
            0,
            ctx,
            &mut seen,
            out,
        );
    }

    reader.take(ctx)?;
    // Root-form disambiguation and lexical validation require one additional
    // classified line. This is the decoder's only two-line lookahead point.
    let _ = reader.peek(ctx)?;
    emit_object_from_first(&mut reader, first, ctx, out)?;
    expect_stream_end(&mut reader, ctx)
}

fn expect_stream_end<R: BufRead>(
    reader: &mut StreamReader<R>,
    ctx: &StreamCtx,
) -> Result<(), ParseError> {
    if let Some(trailing) = reader.peek(ctx)? {
        if ctx.strict {
            return Err(stream_error(trailing.number, "expected end of document"));
        }
    }
    Ok(())
}

// #region Objects (§8)

fn emit_object_from_first<R: BufRead, S: EventSink>(
    reader: &mut StreamReader<R>,
    first: StreamLine,
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<(), ParseError> {
    out.emit(ToonEvent::StartObject { line: first.number })?;
    let mut seen = HashSet::new();
    let content = first.content.clone();
    emit_entry(reader, &first, &content, 0, ctx, &mut seen, out)?;
    loop {
        let line = match reader.peek(ctx)? {
            None => break,
            Some(line) => line,
        };
        if line.depth > 0 {
            return Err(stream_error(line.number, "over-indented line"));
        }
        reader.take(ctx)?;
        let content = line.content.clone();
        emit_entry(reader, &line, &content, 0, ctx, &mut seen, out)?;
    }
    out.emit(ToonEvent::EndObject { line: reader.last_number(first.number) })?;
    Ok(())
}

fn emit_object<R: BufRead, S: EventSink>(
    reader: &mut StreamReader<R>,
    depth: usize,
    start_line: usize,
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<(), ParseError> {
    out.emit(ToonEvent::StartObject { line: start_line })?;
    let mut seen = HashSet::new();
    loop {
        let line = match reader.peek(ctx)? {
            None => break,
            Some(line) => line,
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
    out.emit(ToonEvent::EndObject { line: reader.last_number(start_line) })?;
    Ok(())
}

/// One object entry whose content sits at `depth` (possibly carried on a
/// hyphen line, §10).
#[allow(clippy::too_many_arguments)]
fn emit_entry<R: BufRead, S: EventSink>(
    reader: &mut StreamReader<R>,
    line: &StreamLine,
    content: &str,
    depth: usize,
    ctx: &StreamCtx,
    seen: &mut HashSet<String>,
    out: &mut S,
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
                out.emit(ToonEvent::Key { key: key.clone(), line: line.number })?;
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
    out.emit(ToonEvent::Key { key, line: line.number })?;

    if rest.is_empty() {
        let child = reader.peek(ctx)?;
        if let Some(child) = child {
            if child.depth > depth {
                if child.depth != depth + 1 {
                    return Err(stream_error(child.number, "over-indented line"));
                }
                return emit_object(reader, depth + 1, child.number, ctx, out);
            }
        }
        out.emit(ToonEvent::StartObject { line: line.number })?;
        out.emit(ToonEvent::EndObject { line: line.number })?;
        return Ok(());
    }
    if rest == "[]" {
        out.emit(ToonEvent::StartArray { length: 0, line: line.number })?;
        out.emit(ToonEvent::EndArray { line: line.number })?;
        return Ok(());
    }
    out.emit(ToonEvent::Primitive {
        value: parse_scalar(rest, line.number)?,
        line: line.number,
    })?;
    Ok(())
}

// #endregion

// #region Arrays (§9.1, §9.2, §9.4) and list items (§10)

fn emit_array<R: BufRead, S: EventSink>(
    reader: &mut StreamReader<R>,
    header: &StreamLine,
    info: &StreamHeader,
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<(), ParseError> {
    out.emit(ToonEvent::StartArray { length: info.length, line: header.number })?;

    if let Some(fields) = &info.fields {
        assert_no_duplicate_stream_fields(fields, header.number, ctx)?;
        return emit_tabular_rows(reader, header, info, fields, ctx, out);
    }

    if let Some(inline) = &info.inline {
        let values = split_stream_cells(inline, info.delimiter, header.number);
        assert_stream_count(values.len(), info.length, header.number, ctx)?;
        for value in values {
            out.emit(ToonEvent::Primitive {
                value: parse_scalar(&value, header.number)?,
                line: header.number,
            })?;
        }
        out.emit(ToonEvent::EndArray { line: header.number })?;
        return Ok(());
    }

    // List form: items at depth +1, each `- …` or the bare `-` (§9.4, §10).
    let mut items = 0usize;
    loop {
        let line = match reader.peek(ctx)? {
            None => break,
            Some(line) => line,
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
    out.emit(ToonEvent::EndArray { line: end_line })?;
    Ok(())
}

fn emit_list_item<R: BufRead, S: EventSink>(
    reader: &mut StreamReader<R>,
    line: &StreamLine,
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<(), ParseError> {
    if line.content == "-" {
        out.emit(ToonEvent::StartObject { line: line.number })?;
        out.emit(ToonEvent::EndObject { line: line.number })?;
        return Ok(());
    }
    let trimmed = trim_u0020(&line.content[2..]).to_owned();

    if trimmed == "[]" {
        out.emit(ToonEvent::StartArray { length: 0, line: line.number })?;
        out.emit(ToonEvent::EndArray { line: line.number })?;
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
                out.emit(ToonEvent::Primitive {
                    value: parse_scalar(&trimmed, line.number)?,
                    line: line.number,
                })?;
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
        out.emit(ToonEvent::StartObject { line: line.number })?;
        let mut seen = HashSet::new();
        emit_entry(reader, line, &trimmed, line.depth + 1, ctx, &mut seen, out)?;
        loop {
            let next = match reader.peek(ctx)? {
                None => break,
                Some(next) => next,
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
        out.emit(ToonEvent::EndObject { line: reader.last_number(line.number) })?;
        return Ok(());
    }

    out.emit(ToonEvent::Primitive {
        value: parse_scalar(&trimmed, line.number)?,
        line: line.number,
    })?;
    Ok(())
}

// #endregion

// #region Tabular rows (§9.3)

fn emit_tabular_rows<R: BufRead, S: EventSink>(
    reader: &mut StreamReader<R>,
    header: &StreamLine,
    info: &StreamHeader,
    fields: &[StreamFieldNode],
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<(), ParseError> {
    let leaf_count = count_stream_leaves(fields);
    let row_depth = header.depth + 1;
    let mut rows = 0usize;
    loop {
        let line = match reader.peek(ctx)? {
            None => break,
            Some(line) => line,
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
    out.emit(ToonEvent::EndArray { line: end_line })?;
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

fn emit_row_object<S: EventSink>(
    fields: &[StreamFieldNode],
    cells: &[String],
    cursor: &mut usize,
    line: usize,
    out: &mut S,
) -> Result<(), ParseError> {
    out.emit(ToonEvent::StartObject { line })?;
    for field in fields {
        out.emit(ToonEvent::Key { key: field.name.clone(), line })?;
        match &field.children {
            None => {
                let empty = String::new();
                let cell = cells.get(*cursor).unwrap_or(&empty);
                *cursor += 1;
                out.emit(ToonEvent::Primitive { value: parse_scalar(cell, line)?, line })?;
            }
            Some(children) => emit_row_object(children, cells, cursor, line, out)?,
        }
    }
    out.emit(ToonEvent::EndObject { line })?;
    Ok(())
}

// #endregion

// #region Keyed tabular objects (§9.5)

fn emit_keyed_object<R: BufRead, S: EventSink>(
    reader: &mut StreamReader<R>,
    header: &StreamLine,
    info: &StreamHeader,
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<(), ParseError> {
    let fields = info.fields.as_ref().expect("keyed header always carries fields");
    assert_no_duplicate_stream_fields(fields, header.number, ctx)?;
    let leaf_count = count_stream_leaves(fields);
    let entry_depth = header.depth + 1;
    out.emit(ToonEvent::StartObject { line: header.number })?;
    let mut seen = HashSet::new();
    let mut rows = 0usize;
    loop {
        let line = match reader.peek(ctx)? {
            None => break,
            Some(line) => line,
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
        out.emit(ToonEvent::Key { key, line: line.number })?;
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
    out.emit(ToonEvent::EndObject { line: end_line })?;
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
