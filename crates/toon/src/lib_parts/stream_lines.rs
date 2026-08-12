/// A full-line comment: only U+0020 spaces before `#` (§5.1).
fn is_comment_line(raw: &str) -> bool {
    raw.trim_start_matches(' ').starts_with('#')
}

/// Token trimming is exactly U+0020 (§12).
fn trim_u0020(text: &str) -> &str {
    text.trim_matches(' ')
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
