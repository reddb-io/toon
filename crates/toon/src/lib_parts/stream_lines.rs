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
    // Most lines carry no `{` at all; `contains` on bytes is a memchr scan.
    if max_depth == 0 || !content.as_bytes().contains(&b'{') {
        return Ok(());
    }
    // ASCII structure only, so bytes stand in for chars (see `scan.rs`).
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for &byte in content.as_bytes() {
        if escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if !quoted && byte == b'{' {
            depth += 1;
            if depth > max_depth {
                return Err(stream_depth_error(line, max_depth));
            }
        } else if !quoted && byte == b'}' {
            depth = depth.saturating_sub(1);
        }
    }
    Ok(())
}
