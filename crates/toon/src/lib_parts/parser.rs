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
                max_depth: None,
            });
        }
        if options.strict && spaces % options.indent != 0 {
            return Err(ParseError {
                line: number,
                message: "invalid indentation",
                max_depth: None,
            });
        }

        let depth = spaces / options.indent;
        check_parse_depth(depth, number, options)?;

        lines.push(Line {
            number,
            depth,
            content: &raw_line[spaces..],
            blank_before,
        });
        blank_before = false;
    }

    Ok(lines)
}

fn check_parse_depth(depth: usize, line: usize, options: &ParseOptions) -> Result<(), ParseError> {
    if options.max_depth != 0 && depth > options.max_depth {
        return Err(ParseError {
            line,
            message: "maximum nesting depth exceeded",
            max_depth: Some(options.max_depth),
        });
    }
    Ok(())
}

fn check_header_depth(header: &str, line: usize, options: &ParseOptions) -> Result<(), ParseError> {
    if options.max_depth == 0 {
        return Ok(());
    }

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for character in header.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => {
                depth += 1;
                check_parse_depth(depth, line, options)?;
            }
            '}' if !in_string => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
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
                max_depth: None,
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
        max_depth: None,
    })?;
    let key_part = &content[..colon];
    let value_part = &content[colon + 1..];

    let array_open = find_unquoted(key_part, '[', line.number)?;
    let map_open = find_unquoted(key_part, '{', line.number)?;
    if map_open.is_some_and(|open| array_open.map_or(true, |array_open| open < array_open)) {
        check_header_depth(key_part, line.number, options)?;
        match parse_map_header(key_part) {
            Ok(header) => {
                if !value_part.trim().is_empty() {
                    return Err(ParseError {
                        line: line.number,
                        message: "expected keyed map rows",
                        max_depth: None,
                    });
                }
                let key = header.key.clone();
                let value = parse_keyed_map_rows(&header, lines, index, depth + 1, options)?;
                return Ok((key, header.key_quoted, value));
            }
            Err(error) if options.strict => return Err(error.at(line.number)),
            Err(_) => {
                *index += 1;
                let value =
                    parse_field_value(lines, index, depth, value_part, line.number, options)?;
                return Ok((key_part.trim().to_owned(), false, value));
            }
        }
    }

    if array_open.is_some() {
        check_header_depth(key_part, line.number, options)?;
        match parse_header(key_part, Some(colon)) {
            Ok(header) => {
                if header.key.is_empty() && !header.key_quoted {
                    return Err(ParseError {
                        line: line.number,
                        message: "expected non-empty field name",
                        max_depth: None,
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
            max_depth: None,
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

fn parse_keyed_map_rows(
    header: &MapHeader,
    lines: &[Line<'_>],
    index: &mut usize,
    row_depth: usize,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    let mut document = Document::default();
    *index += 1;

    while let Some(line) = lines.get(*index) {
        if line.depth < row_depth {
            break;
        }
        if line.depth > row_depth {
            return Err(ParseError {
                line: line.number,
                message: "invalid indentation",
                max_depth: None,
            });
        }
        if line.blank_before && options.strict {
            return Err(ParseError {
                line: line.number,
                message: "blank line inside keyed map",
                max_depth: None,
            });
        }

        let colon = find_unquoted(line.content, ':', line.number)?.ok_or(ParseError {
            line: line.number,
            message: "expected `key: value`",
            max_depth: None,
        })?;
        let (key, quoted) = parse_key(&line.content[..colon], line.number)?;
        if key.is_empty() && !quoted {
            return Err(ParseError {
                line: line.number,
                message: "expected non-empty field name",
                max_depth: None,
            });
        }
        let cells = split_delimited(
            line.content[colon + 1..].trim(),
            header.delimiter,
            line.number,
        )?;
        if cells.len() != header.fields.len() {
            return Err(ParseError {
                line: line.number,
                message: "keyed map row length mismatch",
                max_depth: None,
            });
        }

        let mut row = Document::default();
        for (field, cell) in header.fields.iter().zip(cells.iter()) {
            let value = parse_tabular_cell(field, cell, line.number)?;
            let segments = field.path.iter().map(String::as_str).collect::<Vec<_>>();
            insert_path(&mut row, &segments, value, options, line.number)?;
        }
        insert_field(
            &mut document,
            &key,
            quoted,
            Value::Object(row),
            options,
            line.number,
        )?;
        *index += 1;
    }

    Ok(Value::Object(document))
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
        max_depth: None,
    })?;
    let value_part = &first.content[colon + 1..];
    let mut index = 0;
    let value = parse_array_field(&header, value_part, lines, &mut index, 0, options)?;
    if let Some(line) = lines.get(index) {
        return Err(ParseError {
            line: line.number,
            message: "expected end of document",
            max_depth: None,
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
                max_depth: None,
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
                max_depth: None,
            });
        }
        return Ok(Value::Array(Array::List(values)));
    }

    parse_list_items(header, lines, index, header_depth + 1, options)
}

fn parse_tabular_rows(
    header: &Header,
    fields: &[HeaderField],
    lines: &[Line<'_>],
    index: &mut usize,
    row_depth: usize,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    if header
        .field_tree
        .as_ref()
        .is_some_and(|fields| has_complex_header_fields(fields))
    {
        let fields = header.field_tree.as_ref().expect("checked above");
        return parse_structured_tabular_rows(header, fields, lines, index, row_depth, options);
    }

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
                max_depth: None,
            });
        }
        if line.blank_before && options.strict {
            return Err(ParseError {
                line: line.number,
                message: "blank line inside array",
                max_depth: None,
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
                max_depth: None,
            });
        }
        rows.push(
            cells
                .iter()
                .zip(fields.iter())
                .map(|(cell, field)| parse_tabular_cell(field, cell, line.number))
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
                max_depth: None,
            });
        }
    }

    Ok(Value::Array(Array::Tabular(TabularArray {
        fields: fields.to_vec(),
        rows,
    })))
}

fn parse_structured_tabular_rows(
    header: &Header,
    fields: &[HeaderFieldTree],
    lines: &[Line<'_>],
    index: &mut usize,
    row_depth: usize,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    let rows = parse_structured_rows(
        header.len,
        fields,
        header.delimiter,
        lines,
        index,
        row_depth,
        options,
        true,
    )?;
    Ok(Value::Array(Array::List(rows)))
}

struct StructuredState {
    cell_index: usize,
    next_index: usize,
    flat_width: usize,
    child_table_fields: Option<Vec<bool>>,
}

struct ValidationResult {
    next_index: usize,
    consumed_child_rows: usize,
}

#[allow(clippy::too_many_arguments)]
fn parse_structured_rows(
    len: usize,
    fields: &[HeaderFieldTree],
    delimiter: char,
    lines: &[Line<'_>],
    index: &mut usize,
    row_depth: usize,
    options: &ParseOptions,
    root: bool,
) -> Result<Vec<Value>, ParseError> {
    let mut rows = Vec::new();
    let child_table_fields =
        infer_child_table_fields(len, fields, delimiter, lines, *index, row_depth, options);

    while rows.len() < len {
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
                max_depth: None,
            });
        }
        if line.blank_before && options.strict {
            return Err(ParseError {
                line: line.number,
                message: "blank line inside array",
                max_depth: None,
            });
        }
        if !is_tabular_row(line.content, delimiter, line.number)? {
            break;
        }

        let cells = split_delimited(line.content, delimiter, line.number)?;
        let mut state = StructuredState {
            cell_index: 0,
            next_index: *index + 1,
            flat_width: leaf_width(fields),
            child_table_fields: child_table_fields.clone(),
        };
        let row = parse_structured_row_fields(
            fields,
            &cells,
            line.number,
            lines,
            &mut state,
            row_depth + 1,
            delimiter,
            options,
        )?;
        if state.cell_index != cells.len() {
            return Err(ParseError {
                line: line.number,
                message: "array row length mismatch",
                max_depth: None,
            });
        }
        rows.push(matrix_row_value(fields, row, root));
        *index = state.next_index;
    }

    if rows.len() != len {
        return Err(length_mismatch(lines, *index));
    }
    if let Some(line) = lines.get(*index) {
        if line.depth >= row_depth && is_tabular_row(line.content, delimiter, line.number)? {
            return Err(ParseError {
                line: line.number,
                message: "array length mismatch",
                max_depth: None,
            });
        }
    }

    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
fn parse_structured_row_fields(
    fields: &[HeaderFieldTree],
    cells: &[String],
    line: usize,
    lines: &[Line<'_>],
    state: &mut StructuredState,
    child_depth: usize,
    delimiter: char,
    options: &ParseOptions,
) -> Result<Document, ParseError> {
    let mut row = Document::default();
    for (field_index, field) in fields.iter().enumerate() {
        let remaining_fields = &fields[field_index + 1..];
        let known_child_table = state
            .child_table_fields
            .as_ref()
            .and_then(|fields| fields.get(field_index))
            .copied();
        let value = parse_structured_field(
            field,
            remaining_fields,
            known_child_table,
            cells,
            line,
            lines,
            state,
            child_depth,
            delimiter,
            options,
        )?;
        insert_path(&mut row, &[field.key.as_str()], value, options, line)?;
    }
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
fn parse_structured_field(
    field: &HeaderFieldTree,
    remaining_fields: &[HeaderFieldTree],
    known_child_table: Option<bool>,
    cells: &[String],
    line: usize,
    lines: &[Line<'_>],
    state: &mut StructuredState,
    child_depth: usize,
    delimiter: char,
    options: &ParseOptions,
) -> Result<Value, ParseError> {
    if let Some(fixed_len) = field.fixed_len {
        if state.cell_index + fixed_len > cells.len() {
            return Err(ParseError {
                line,
                message: "array row length mismatch",
                max_depth: None,
            });
        }
        let values = cells[state.cell_index..state.cell_index + fixed_len]
            .iter()
            .map(|cell| parse_scalar(cell, line))
            .collect::<Result<Vec<_>, _>>()?;
        state.cell_index += fixed_len;
        return Ok(Value::Array(Array::List(values)));
    }

    if !field.children.is_empty() {
        let flat_width = leaf_width(&field.children);
        let count = cells
            .get(state.cell_index)
            .and_then(|cell| parse_child_count(cell));
        let cells_after_child_count = cells.len().saturating_sub(state.cell_index + 1);
        let has_child_rows = lines
            .get(state.next_index)
            .is_some_and(|line| line.depth == child_depth);
        let must_be_child_table = known_child_table.unwrap_or_else(|| {
            count.is_some()
                && (has_child_rows
                    || (cells.len() != state.flat_width
                        && cells_after_child_count
                            < flat_width + minimum_row_width(remaining_fields)))
        });

        if must_be_child_table {
            let Some(count) = count else {
                return Err(ParseError {
                    line,
                    message: "array row length mismatch",
                    max_depth: None,
                });
            };
            state.cell_index += 1;
            let mut child_index = state.next_index;
            let rows = parse_structured_rows(
                count,
                &field.children,
                delimiter,
                lines,
                &mut child_index,
                child_depth,
                options,
                false,
            )?;
            state.next_index = child_index;
            return Ok(Value::Array(Array::List(rows)));
        }

        let nested = parse_structured_row_fields(
            &field.children,
            cells,
            line,
            lines,
            state,
            child_depth,
            delimiter,
            options,
        )?;
        return Ok(Value::Object(nested));
    }

    let Some(cell) = cells.get(state.cell_index) else {
        return Err(ParseError {
            line,
            message: "array row length mismatch",
            max_depth: None,
        });
    };
    state.cell_index += 1;
    parse_tabular_cell(
        &HeaderField {
            path: vec![field.key.clone()],
            list_delimiter: field.list_delimiter,
        },
        cell,
        line,
    )
}

#[allow(clippy::too_many_arguments)]
fn infer_child_table_fields(
    len: usize,
    fields: &[HeaderFieldTree],
    delimiter: char,
    lines: &[Line<'_>],
    start_index: usize,
    row_depth: usize,
    options: &ParseOptions,
) -> Option<Vec<bool>> {
    let candidates = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| (!field.children.is_empty()).then_some(index))
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
        let Some(result) = validate_structured_rows_with_kind(
            len,
            fields,
            &child_table_fields,
            delimiter,
            lines,
            start_index,
            row_depth,
            options,
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

    best.map(|(child_table_fields, _, _)| child_table_fields)
}

#[allow(clippy::too_many_arguments)]
fn validate_structured_rows_with_kind(
    len: usize,
    fields: &[HeaderFieldTree],
    child_table_fields: &[bool],
    delimiter: char,
    lines: &[Line<'_>],
    start_index: usize,
    row_depth: usize,
    options: &ParseOptions,
) -> Option<ValidationResult> {
    let mut index = start_index;
    let mut consumed_child_rows = 0;

    for _ in 0..len {
        let line = lines.get(index)?;
        if line.depth != row_depth
            || (line.blank_before && options.strict)
            || !is_tabular_row(line.content, delimiter, line.number).ok()?
        {
            return None;
        }

        let cells = split_delimited(line.content, delimiter, line.number).ok()?;
        let result = validate_structured_row_with_kind(
            fields,
            child_table_fields,
            &cells,
            delimiter,
            lines,
            index + 1,
            row_depth + 1,
            options,
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

    if let Some(line) = lines.get(index) {
        if line.depth >= row_depth && is_tabular_row(line.content, delimiter, line.number).ok()? {
            return None;
        }
    }

    Some(ValidationResult {
        next_index: index,
        consumed_child_rows,
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_structured_row_with_kind(
    fields: &[HeaderFieldTree],
    child_table_fields: &[bool],
    cells: &[String],
    delimiter: char,
    lines: &[Line<'_>],
    start_index: usize,
    child_depth: usize,
    options: &ParseOptions,
) -> Option<ValidationResult> {
    let mut cell_index = 0;
    let mut next_index = start_index;
    let mut consumed_child_rows = 0;

    for (field_index, field) in fields.iter().enumerate() {
        if let Some(fixed_len) = field.fixed_len {
            cell_index += fixed_len;
        } else if !field.children.is_empty() {
            if child_table_fields
                .get(field_index)
                .copied()
                .unwrap_or(false)
            {
                let count = cells
                    .get(cell_index)
                    .and_then(|cell| parse_child_count(cell))?;
                cell_index += 1;
                let nested_child_table_fields = infer_child_table_fields(
                    count,
                    &field.children,
                    delimiter,
                    lines,
                    next_index,
                    child_depth,
                    options,
                )?;
                let result = validate_structured_rows_with_kind(
                    count,
                    &field.children,
                    &nested_child_table_fields,
                    delimiter,
                    lines,
                    next_index,
                    child_depth,
                    options,
                )?;
                next_index = result.next_index;
                consumed_child_rows += count + result.consumed_child_rows;
            } else {
                cell_index += leaf_width(&field.children);
            }
        } else {
            cell_index += 1;
        }

        if cell_index > cells.len() {
            return None;
        }
    }

    if cell_index != cells.len() {
        return None;
    }

    Some(ValidationResult {
        next_index,
        consumed_child_rows,
    })
}

fn matrix_row_value(fields: &[HeaderFieldTree], row: Document, root: bool) -> Value {
    if root && fields.len() == 1 && fields[0].fixed_len.is_some() {
        return row
            .get(&fields[0].key)
            .cloned()
            .expect("structured row inserted fixed-width field");
    }
    Value::Object(row)
}

fn parse_child_count(value: &str) -> Option<usize> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}

fn has_complex_header_fields(fields: &[HeaderFieldTree]) -> bool {
    fields.iter().any(|field| {
        field.fixed_len.is_some()
            || !field.children.is_empty()
            || has_complex_header_fields(&field.children)
    })
}

fn leaf_width(fields: &[HeaderFieldTree]) -> usize {
    fields.iter().map(field_width).sum()
}

fn field_width(field: &HeaderFieldTree) -> usize {
    if let Some(fixed_len) = field.fixed_len {
        return fixed_len;
    }
    if !field.children.is_empty() {
        return leaf_width(&field.children);
    }
    1
}

fn minimum_row_width(fields: &[HeaderFieldTree]) -> usize {
    fields
        .iter()
        .map(|field| {
            if !field.children.is_empty() {
                1
            } else {
                field_width(field)
            }
        })
        .sum()
}

fn parse_tabular_cell(field: &HeaderField, cell: &str, line: usize) -> Result<Value, ParseError> {
    let Some(list_delimiter) = field.list_delimiter else {
        return parse_scalar(cell, line);
    };
    let values = split_delimited(cell, list_delimiter, line)?
        .iter()
        .map(|value| parse_scalar(value, line))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Array(Array::List(values)))
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
                max_depth: None,
            });
        }
        if line.blank_before && options.strict {
            return Err(ParseError {
                line: line.number,
                message: "blank line inside array",
                max_depth: None,
            });
        }
        values.push(parse_list_item(lines, index, item_depth, options)?);
    }

    if let Some(line) = lines.get(*index) {
        if line.depth >= item_depth {
            return Err(ParseError {
                line: line.number,
                message: "array length mismatch",
                max_depth: None,
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
            max_depth: None,
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
        check_header_depth(inner, line.number, options)?;
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
        max_depth: None,
    }
}

