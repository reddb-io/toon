// Byte-level scanners for the quote-aware hot loops. Every structural
// character TOON looks for (delimiters, `"`, `\`, `:`) is ASCII, and in UTF-8 an
// ASCII byte never occurs inside a multi-byte character, so scanning bytes
// finds exactly the positions a `char` walk finds without decoding UTF-8. A
// non-ASCII needle (possible only for an exotic list delimiter) keeps the
// `char` walk in the callers.

/// `u8` form of a needle when it is ASCII.
fn ascii_needle(needle: char) -> Option<u8> {
    needle.is_ascii().then_some(needle as u8)
}

/// [`split_stream_cells`] for an ASCII delimiter: a `"` opens a quoted run
/// that only an unescaped `"` closes, and an unterminated run is lenient.
fn split_stream_cells_ascii(content: &str, delimiter: u8) -> Vec<&str> {
    let bytes = content.as_bytes();
    let mut cells = Vec::new();
    let mut start = 0usize;
    let mut in_quotes = false;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_quotes {
            match byte {
                b'\\' => index += 1,
                b'"' => in_quotes = false,
                _ => {}
            }
        } else if byte == b'"' {
            in_quotes = true;
        } else if byte == delimiter {
            cells.push(trim_u0020(&content[start..index]));
            start = index + 1;
        }
        index += 1;
    }
    cells.push(trim_u0020(&content[start..]));
    cells
}

/// Walks `value` outside quoted runs, calling `on_needle` at each unquoted
/// occurrence of `needle` until it returns `false`. Returns whether a quoted
/// run was left open, the condition the strict splitters reject.
fn scan_unquoted_ascii(value: &str, needle: u8, mut on_needle: impl FnMut(usize) -> bool) -> bool {
    let bytes = value.as_bytes();
    let mut in_string = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if in_string => {
                if index + 1 == bytes.len() {
                    return true;
                }
                index += 1;
            }
            b'"' => in_string = !in_string,
            byte if byte == needle && !in_string && !on_needle(index) => return false,
            _ => {}
        }
        index += 1;
    }
    in_string
}
