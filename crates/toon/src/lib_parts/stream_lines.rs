/// A full-line comment: only U+0020 spaces before `#` (§5.1).
fn is_comment_line(raw: &str) -> bool {
    raw.trim_start_matches(' ').starts_with('#')
}

/// Token trimming is exactly U+0020 (§12).
fn trim_u0020(text: &str) -> &str {
    text.trim_matches(' ')
}

/// Whole-input classification remains solely for truncation diagnostics. The
/// event decoder classifies through `StreamReader` one line at a time.
fn classify_stream_lines(input: &str, ctx: &StreamCtx) -> Result<Vec<StreamLine>, ParseError> {
    let mut lines = Vec::new();
    let mut blank_pending = false;
    for (index, raw_line) in input.split('\n').enumerate() {
        let number = index + 1;
        let mut raw = raw_line;
        if number == 1 {
            raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
        }
        raw = raw.strip_suffix('\r').unwrap_or(raw);
        raw = raw.trim_end_matches(' ');
        if raw.trim().is_empty() {
            blank_pending = true;
            continue;
        }
        if is_comment_line(raw) {
            continue;
        }
        let mut offset = 0;
        let mut tabs = 0usize;
        let mut spaces = 0usize;
        for character in raw.chars() {
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
        check_stream_header_depth(&raw[offset..], number, ctx.max_depth)?;
        lines.push(StreamLine {
            number,
            depth,
            content: raw[offset..].to_owned(),
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
    let mut depth = 0usize;
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
