//! Token estimates compatible with tokenx 1.3.0, the estimator used by the
//! pinned upstream TOON CLI. Both Rust front-ends read `--stats` from here —
//! `tq` and the `toon` bin — so a single engine decides the numbers, and
//! `packages/toon/src/cli/tokens.ts` is its TypeScript twin.

/// Formats the `--stats` report, matching upstream wording and rounding.
pub fn format_statistics(json: &str, toon: &str) -> String {
    let json_tokens = estimate_token_count(json);
    let toon_tokens = estimate_token_count(toon);
    let difference = json_tokens as i64 - toon_tokens as i64;
    let percent = difference as f64 / json_tokens as f64 * 100.0;

    format!(
        "● Token estimates: ~{json_tokens} (JSON) → ~{toon_tokens} (TOON)\n\
         ✔ Saved ~{difference} tokens (-{percent:.1}%)\n"
    )
}

/// Estimates the token count of `text` the way tokenx 1.3.0 does.
pub fn estimate_token_count(text: &str) -> usize {
    let mut rest = text;
    let mut total = 0;

    while let Some(first) = rest.chars().next() {
        if is_javascript_whitespace(first) {
            rest = take_segment(rest, is_javascript_whitespace).1;
        } else if is_punctuation(first) {
            let (segment, remaining) = take_segment(rest, is_punctuation);
            total += segment.encode_utf16().count().div_ceil(2);
            rest = remaining;
        } else {
            let (segment, remaining) = take_segment(rest, |character| {
                !is_javascript_whitespace(character) && !is_punctuation(character)
            });
            total += estimate_segment(segment);
            rest = remaining;
        }
    }

    total
}

fn take_segment(text: &str, matches: impl Fn(char) -> bool) -> (&str, &str) {
    let end = text
        .char_indices()
        .find_map(|(index, character)| (!matches(character)).then_some(index))
        .unwrap_or(text.len());
    text.split_at(end)
}

fn estimate_segment(segment: &str) -> usize {
    if segment.chars().any(is_cjk) {
        return segment.chars().count();
    }
    if segment.chars().all(|character| character.is_ascii_digit()) {
        return 1;
    }

    let length = segment.encode_utf16().count();
    if length <= 3 {
        return 1;
    }

    match language_chars_per_token(segment) {
        Some(3) => length.div_ceil(3),
        Some(7) => (length * 2).div_ceil(7),
        _ => length.div_ceil(6),
    }
}

fn language_chars_per_token(segment: &str) -> Option<usize> {
    if segment.chars().any(is_german) || segment.chars().any(is_romance) {
        Some(3)
    } else if segment.chars().any(is_central_european) {
        // `7` represents tokenx's 3.5 characters per token without floats.
        Some(7)
    } else {
        None
    }
}

fn is_javascript_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'..='\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

fn is_punctuation(character: char) -> bool {
    matches!(
        character,
        '.' | ','
            | '!'
            | '?'
            | ';'
            | '('
            | ')'
            | '{'
            | '}'
            | '['
            | ']'
            | '<'
            | '>'
            | ':'
            | '/'
            | '\\'
            | '|'
            | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '+'
            | '='
            | '`'
            | '~'
            | '_'
            | '-'
    )
}

fn is_cjk(character: char) -> bool {
    matches!(
        character,
        '\u{4E00}'..='\u{9FFF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{3000}'..='\u{303F}'
            | '\u{FF00}'..='\u{FFEF}'
            | '\u{30A0}'..='\u{30FF}'
            | '\u{2E80}'..='\u{2EFF}'
            | '\u{31C0}'..='\u{31EF}'
            | '\u{3200}'..='\u{32FF}'
            | '\u{3300}'..='\u{33FF}'
            | '\u{AC00}'..='\u{D7AF}'
            | '\u{1100}'..='\u{11FF}'
            | '\u{3130}'..='\u{318F}'
            | '\u{A960}'..='\u{A97F}'
            | '\u{D7B0}'..='\u{D7FF}'
    )
}

fn is_german(character: char) -> bool {
    matches!(character, 'ä' | 'ö' | 'ü' | 'ß' | 'ẞ' | 'Ä' | 'Ö' | 'Ü')
}

fn is_romance(character: char) -> bool {
    matches!(
        character.to_lowercase().next().unwrap_or(character),
        'é' | 'è'
            | 'ê'
            | 'ë'
            | 'à'
            | 'â'
            | 'î'
            | 'ï'
            | 'ô'
            | 'û'
            | 'ù'
            | 'ü'
            | 'ÿ'
            | 'ç'
            | 'œ'
            | 'æ'
            | 'á'
            | 'í'
            | 'ó'
            | 'ú'
            | 'ñ'
    )
}

fn is_central_european(character: char) -> bool {
    matches!(
        character.to_lowercase().next().unwrap_or(character),
        'ą' | 'ć'
            | 'ę'
            | 'ł'
            | 'ń'
            | 'ó'
            | 'ś'
            | 'ź'
            | 'ż'
            | 'ě'
            | 'š'
            | 'č'
            | 'ř'
            | 'ž'
            | 'ý'
            | 'ů'
            | 'ú'
            | 'ď'
            | 'ť'
            | 'ň'
    )
}
