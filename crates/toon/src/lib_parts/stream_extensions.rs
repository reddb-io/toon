// Event emission for mixed-column tabular extensions. Complex tables are
// buffered as one span because a zero-count child cell is ambiguous until a
// later row establishes whether the field is an inline object or child table.

fn parse_stream_field_list(
    text: &str,
    delimiter: char,
    active_delimiter: char,
    line: usize,
) -> Result<Vec<StreamFieldNode>, ParseError> {
    let mut entries = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut bracket_depth = 0i32;
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
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            c if c == delimiter && depth == 0 && bracket_depth == 0 => {
                entries.push(parse_stream_field_entry(
                    &text[start..i],
                    delimiter,
                    active_delimiter,
                    line,
                )?);
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    entries.push(parse_stream_field_entry(
        &text[start..],
        delimiter,
        active_delimiter,
        line,
    )?);
    Ok(entries)
}

fn parse_stream_field_entry(
    chunk: &str,
    delimiter: char,
    active_delimiter: char,
    line: usize,
) -> Result<StreamFieldNode, ParseError> {
    let trimmed = trim_u0020(chunk);
    if trimmed.is_empty() {
        return Err(stream_error(line, "empty field entry in header"));
    }
    let marker_start = if trimmed.starts_with('"') {
        let closing = stream_closing_quote(trimmed, line)?;
        trimmed[closing..]
            .find(['{', '['])
            .map(|offset| closing + offset)
    } else {
        trimmed.find(['{', '['])
    };
    let marker_start = match marker_start {
        Some(index) => index,
        None => {
            return Ok(StreamFieldNode {
                name: parse_key(trimmed, line)?.0,
                children: None,
                list_delimiter: None,
                fixed_len: None,
            });
        }
    };
    let name = parse_key(&trimmed[..marker_start], line)?.0;
    if trimmed.as_bytes()[marker_start] == b'[' {
        if !trimmed.ends_with(']') {
            return Err(stream_error(line, "invalid array header"));
        }
        let bracket = &trimmed[marker_start + 1..trimmed.len() - 1];
        if let Some((fixed_len, fixed_delimiter)) = parse_fixed_width_list(bracket) {
            if fixed_delimiter != active_delimiter {
                return Err(stream_error(line, "invalid array header"));
            }
            return Ok(StreamFieldNode {
                name,
                children: None,
                list_delimiter: None,
                fixed_len: Some(fixed_len),
            });
        }
        let list_delimiter = valid_list_delimiter(bracket, active_delimiter)
            .ok_or_else(|| stream_error(line, "invalid array header"))?;
        return Ok(StreamFieldNode {
            name,
            children: None,
            list_delimiter: Some(list_delimiter),
            fixed_len: None,
        });
    }

    let end = match_stream_brace(&trimmed[marker_start..], line)? + marker_start;
    if end != trimmed.len() - 1 {
        return Err(stream_error(line, "malformed tabular header fields"));
    }
    if end == marker_start + 1 {
        return Err(stream_error(
            line,
            if delimiter == active_delimiter {
                "empty field entry in header"
            } else {
                "invalid array header"
            },
        ));
    }
    let children = parse_stream_field_list(
        &trimmed[marker_start + 1..end],
        delimiter,
        active_delimiter,
        line,
    )?;
    if children.is_empty() {
        return Err(stream_error(line, "empty field entry in header"));
    }
    Ok(StreamFieldNode {
        name,
        children: Some(children),
        list_delimiter: None,
        fixed_len: None,
    })
}

struct StreamStructuredState {
    cell_index: usize,
    next_index: usize,
    flat_width: usize,
    child_table_fields: Option<Vec<bool>>,
    extension_rows: bool,
}

struct StreamValidationResult {
    next_index: usize,
    consumed_child_rows: usize,
}

fn has_complex_stream_fields(fields: &[StreamFieldNode]) -> bool {
    fields.iter().any(|field| {
        field.list_delimiter.is_some() || field.fixed_len.is_some() || field.children.is_some()
    })
}

fn has_fixed_stream_fields(fields: &[StreamFieldNode]) -> bool {
    fields.iter().any(|field| {
        field.fixed_len.is_some()
            || field
                .children
                .as_deref()
                .is_some_and(has_fixed_stream_fields)
    })
}

fn has_typed_stream_fields(fields: &[StreamFieldNode]) -> bool {
    fields.iter().any(|field| {
        field.list_delimiter.is_some()
            || field.fixed_len.is_some()
            || field
                .children
                .as_deref()
                .is_some_and(has_typed_stream_fields)
    })
}

fn stream_field_width(field: &StreamFieldNode) -> usize {
    if let Some(fixed_len) = field.fixed_len {
        fixed_len
    } else if let Some(children) = &field.children {
        stream_leaf_width(children)
    } else {
        1
    }
}

fn stream_leaf_width(fields: &[StreamFieldNode]) -> usize {
    fields.iter().map(stream_field_width).sum()
}

fn stream_minimum_row_width(fields: &[StreamFieldNode]) -> usize {
    fields
        .iter()
        .map(|field| {
            if field.children.is_some() {
                1
            } else {
                stream_field_width(field)
            }
        })
        .sum()
}

fn parse_stream_child_count(value: &str) -> Option<usize> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}

fn infer_stream_child_table_fields(
    len: usize,
    fields: &[StreamFieldNode],
    delimiter: char,
    lines: &[StreamLine],
    start_index: usize,
    row_depth: usize,
    ctx: &StreamCtx,
) -> Option<Vec<bool>> {
    let candidates = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| field.children.is_some().then_some(index))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Some(vec![false; fields.len()]);
    }
    if candidates.len() > 12 {
        return None;
    }

    let mut best: Option<(Vec<bool>, usize, usize)> = None;
    for mask in 0..(1usize << candidates.len()) {
        let mut child_table_fields = vec![false; fields.len()];
        for (candidate_offset, field_index) in candidates.iter().enumerate() {
            if mask & (1usize << candidate_offset) != 0 {
                child_table_fields[*field_index] = true;
            }
        }
        let Some(result) = validate_stream_rows_with_kind(
            len,
            fields,
            &child_table_fields,
            delimiter,
            lines,
            start_index,
            row_depth,
            ctx,
        ) else {
            continue;
        };
        let enabled = child_table_fields
            .iter()
            .filter(|enabled| **enabled)
            .count();
        if best.as_ref().map_or(true, |(_, consumed, best_enabled)| {
            result.consumed_child_rows > *consumed
                || (result.consumed_child_rows == *consumed && enabled < *best_enabled)
        }) {
            best = Some((child_table_fields, result.consumed_child_rows, enabled));
        }
    }
    best.map(|(fields, _, _)| fields)
}

#[allow(clippy::too_many_arguments)]
fn validate_stream_rows_with_kind(
    len: usize,
    fields: &[StreamFieldNode],
    child_table_fields: &[bool],
    delimiter: char,
    lines: &[StreamLine],
    start_index: usize,
    row_depth: usize,
    ctx: &StreamCtx,
) -> Option<StreamValidationResult> {
    let mut index = start_index;
    let mut consumed_child_rows = 0;
    for _ in 0..len {
        let line = lines.get(index)?;
        if line.depth != row_depth
            || (line.blank_before && ctx.strict)
            || !is_stream_row(&line.content, delimiter, line.number).ok()?
        {
            return None;
        }
        let cells = split_delimited(&line.content, delimiter, line.number).ok()?;
        let result = validate_stream_row_with_kind(
            fields,
            child_table_fields,
            &cells,
            delimiter,
            lines,
            index + 1,
            row_depth + 1,
            ctx,
        )?;
        if lines
            .get(result.next_index)
            .is_some_and(|line| line.depth > row_depth)
        {
            return None;
        }
        consumed_child_rows += result.consumed_child_rows;
        index = result.next_index;
    }
    if lines.get(index).is_some_and(|line| line.depth >= row_depth) {
        return None;
    }
    Some(StreamValidationResult {
        next_index: index,
        consumed_child_rows,
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_stream_row_with_kind(
    fields: &[StreamFieldNode],
    child_table_fields: &[bool],
    cells: &[String],
    delimiter: char,
    lines: &[StreamLine],
    start_index: usize,
    child_depth: usize,
    ctx: &StreamCtx,
) -> Option<StreamValidationResult> {
    let mut cell_index = 0;
    let mut next_index = start_index;
    let mut consumed_child_rows = 0;
    for (field_index, field) in fields.iter().enumerate() {
        if let Some(fixed_len) = field.fixed_len {
            cell_index += fixed_len;
        } else if let Some(children) = &field.children {
            if child_table_fields
                .get(field_index)
                .copied()
                .unwrap_or(false)
            {
                let count = cells
                    .get(cell_index)
                    .and_then(|cell| parse_stream_child_count(cell))?;
                cell_index += 1;
                let nested_kinds = infer_stream_child_table_fields(
                    count,
                    children,
                    delimiter,
                    lines,
                    next_index,
                    child_depth,
                    ctx,
                )?;
                let result = validate_stream_rows_with_kind(
                    count,
                    children,
                    &nested_kinds,
                    delimiter,
                    lines,
                    next_index,
                    child_depth,
                    ctx,
                )?;
                next_index = result.next_index;
                consumed_child_rows += count + result.consumed_child_rows;
            } else {
                cell_index += stream_leaf_width(children);
            }
        } else {
            cell_index += 1;
        }
        if cell_index > cells.len() {
            return None;
        }
    }
    (cell_index == cells.len()).then_some(StreamValidationResult {
        next_index,
        consumed_child_rows,
    })
}

fn structured_stream_length_error(
    lines: &[StreamLine],
    index: usize,
    fallback: usize,
) -> ParseError {
    let line = lines
        .get(index)
        .or_else(|| lines.last())
        .map_or(fallback, |line| line.number);
    stream_error(line, "array length mismatch")
}

fn structured_stream_row_error(line: usize, extension_rows: bool) -> ParseError {
    stream_error(
        line,
        if extension_rows {
            "array row length mismatch"
        } else {
            "array count mismatch"
        },
    )
}

fn emit_complex_tabular_rows<R: BufRead, S: EventSink>(
    reader: &mut StreamReader<R>,
    header: &StreamLine,
    info: &StreamHeader,
    fields: &[StreamFieldNode],
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<(), ParseError> {
    if !ctx.object_array_columns && has_fixed_stream_fields(fields) {
        return Err(stream_error(header.number, "invalid array header"));
    }
    let mut lines = Vec::new();
    reader.span_active += 1;
    while let Some(line) = reader.peek(ctx)? {
        if line.depth <= header.depth {
            break;
        }
        lines.push(reader.take(ctx)?);
    }
    reader.span_active -= 1;

    let mut index = 0;
    let end_line = emit_stream_structured_rows(
        info.length,
        fields,
        info.delimiter,
        &lines,
        &mut index,
        header.depth + 1,
        ctx,
        out,
        true,
        header.number,
    )?;
    if index != lines.len() && ctx.strict {
        return Err(structured_stream_length_error(&lines, index, header.number));
    }
    out.emit(ToonEvent::EndArray { line: end_line })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_stream_structured_rows<S: EventSink>(
    len: usize,
    fields: &[StreamFieldNode],
    delimiter: char,
    lines: &[StreamLine],
    index: &mut usize,
    row_depth: usize,
    ctx: &StreamCtx,
    out: &mut S,
    root: bool,
    fallback_line: usize,
) -> Result<usize, ParseError> {
    let child_table_fields = if ctx.object_array_columns {
        infer_stream_child_table_fields(len, fields, delimiter, lines, *index, row_depth, ctx)
    } else {
        Some(vec![false; fields.len()])
    };
    let extension_rows = has_typed_stream_fields(fields)
        || child_table_fields
            .as_ref()
            .is_some_and(|fields| fields.iter().any(|field| *field));
    let mut end_line = fallback_line;
    let mut rows = 0;
    while rows < len {
        let Some(line) = lines.get(*index) else {
            break;
        };
        if line.depth != row_depth || !is_stream_row(&line.content, delimiter, line.number)? {
            break;
        }
        if line.blank_before && ctx.strict {
            return Err(stream_error(line.number, "blank line inside a header span"));
        }
        let cells = split_delimited(&line.content, delimiter, line.number)?;
        let mut state = StreamStructuredState {
            cell_index: 0,
            next_index: *index + 1,
            flat_width: stream_leaf_width(fields),
            child_table_fields: child_table_fields.clone(),
            extension_rows,
        };
        end_line = emit_stream_structured_row(
            fields,
            &cells,
            line.number,
            lines,
            &mut state,
            row_depth + 1,
            delimiter,
            ctx,
            out,
            root,
        )?;
        if state.cell_index != cells.len() {
            return Err(structured_stream_row_error(
                line.number,
                state.extension_rows,
            ));
        }
        *index = state.next_index;
        rows += 1;
    }
    if ctx.strict && rows != len {
        return Err(structured_stream_length_error(lines, *index, fallback_line));
    }
    if ctx.strict
        && lines
            .get(*index)
            .is_some_and(|line| line.depth >= row_depth)
    {
        return Err(structured_stream_length_error(lines, *index, fallback_line));
    }
    Ok(end_line)
}

#[allow(clippy::too_many_arguments)]
fn emit_stream_structured_row<S: EventSink>(
    fields: &[StreamFieldNode],
    cells: &[String],
    line: usize,
    lines: &[StreamLine],
    state: &mut StreamStructuredState,
    child_depth: usize,
    delimiter: char,
    ctx: &StreamCtx,
    out: &mut S,
    root: bool,
) -> Result<usize, ParseError> {
    if root && fields.len() == 1 && fields[0].fixed_len.is_some() {
        return emit_stream_structured_field(
            &fields[0],
            &[],
            Some(false),
            cells,
            line,
            lines,
            state,
            child_depth,
            delimiter,
            ctx,
            out,
        );
    }
    out.emit(ToonEvent::StartObject { line })?;
    let mut end_line = line;
    for (field_index, field) in fields.iter().enumerate() {
        out.emit(ToonEvent::Key {
            key: field.name.clone(),
            line,
        })?;
        let known_child_table = state
            .child_table_fields
            .as_ref()
            .and_then(|kinds| kinds.get(field_index))
            .copied();
        end_line = end_line.max(emit_stream_structured_field(
            field,
            &fields[field_index + 1..],
            known_child_table,
            cells,
            line,
            lines,
            state,
            child_depth,
            delimiter,
            ctx,
            out,
        )?);
    }
    out.emit(ToonEvent::EndObject { line: end_line })?;
    Ok(end_line)
}

#[allow(clippy::too_many_arguments)]
fn emit_stream_structured_field<S: EventSink>(
    field: &StreamFieldNode,
    remaining_fields: &[StreamFieldNode],
    known_child_table: Option<bool>,
    cells: &[String],
    line: usize,
    lines: &[StreamLine],
    state: &mut StreamStructuredState,
    child_depth: usize,
    delimiter: char,
    ctx: &StreamCtx,
    out: &mut S,
) -> Result<usize, ParseError> {
    if let Some(fixed_len) = field.fixed_len {
        if state.cell_index + fixed_len > cells.len() {
            return Err(structured_stream_row_error(line, state.extension_rows));
        }
        out.emit(ToonEvent::StartArray {
            length: fixed_len,
            line,
        })?;
        for cell in &cells[state.cell_index..state.cell_index + fixed_len] {
            out.emit(ToonEvent::Primitive {
                value: parse_scalar(cell, line)?,
                line,
            })?;
        }
        state.cell_index += fixed_len;
        out.emit(ToonEvent::EndArray { line })?;
        return Ok(line);
    }

    if let Some(children) = &field.children {
        let flat_width = stream_leaf_width(children);
        let count = cells
            .get(state.cell_index)
            .and_then(|cell| parse_stream_child_count(cell));
        let cells_after_count = cells.len().saturating_sub(state.cell_index + 1);
        let has_child_rows = lines
            .get(state.next_index)
            .is_some_and(|line| line.depth == child_depth);
        let child_table = known_child_table.unwrap_or_else(|| {
            count.is_some()
                && ctx.object_array_columns
                && (has_child_rows
                    || (cells.len() != state.flat_width
                        && cells_after_count
                            < flat_width + stream_minimum_row_width(remaining_fields)))
        });
        if child_table {
            let count =
                count.ok_or_else(|| structured_stream_row_error(line, state.extension_rows))?;
            state.cell_index += 1;
            out.emit(ToonEvent::StartArray {
                length: count,
                line,
            })?;
            let mut child_index = state.next_index;
            let end_line = emit_stream_structured_rows(
                count,
                children,
                delimiter,
                lines,
                &mut child_index,
                child_depth,
                ctx,
                out,
                false,
                line,
            )?;
            state.next_index = child_index;
            out.emit(ToonEvent::EndArray { line: end_line })?;
            return Ok(end_line);
        }

        out.emit(ToonEvent::StartObject { line })?;
        let previous_kinds = state.child_table_fields.take();
        let mut end_line = line;
        for child in children {
            out.emit(ToonEvent::Key {
                key: child.name.clone(),
                line,
            })?;
            end_line = end_line.max(emit_stream_structured_field(
                child,
                &[],
                None,
                cells,
                line,
                lines,
                state,
                child_depth,
                delimiter,
                ctx,
                out,
            )?);
        }
        state.child_table_fields = previous_kinds;
        out.emit(ToonEvent::EndObject { line: end_line })?;
        return Ok(end_line);
    }

    let cell = cells
        .get(state.cell_index)
        .ok_or_else(|| structured_stream_row_error(line, state.extension_rows))?;
    state.cell_index += 1;
    if let Some(list_delimiter) = field.list_delimiter {
        let values = split_delimited(cell, list_delimiter, line)?;
        out.emit(ToonEvent::StartArray {
            length: values.len(),
            line,
        })?;
        for value in values {
            out.emit(ToonEvent::Primitive {
                value: parse_scalar(&value, line)?,
                line,
            })?;
        }
        out.emit(ToonEvent::EndArray { line })?;
    } else {
        out.emit(ToonEvent::Primitive {
            value: parse_scalar(cell, line)?,
            line,
        })?;
    }
    Ok(line)
}
