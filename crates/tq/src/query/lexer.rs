use super::ast::{AssignOp, BinaryOp};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    Alternative,
    Assign(AssignOp),
    Colon,
    Comma,
    Dot,
    DotDot,
    EqualEqual,
    Greater,
    GreaterEqual,
    FieldDot,
    Format(String),
    Ident(String),
    LBrace,
    LBracket,
    LParen,
    Less,
    LessEqual,
    Minus,
    NotEqual,
    Number(String),
    Pipe,
    Plus,
    Percent,
    Question,
    RBrace,
    RBracket,
    RParen,
    Semicolon,
    Slash,
    Star,
    String(Vec<StringPart>),
    Variable(String),
}

/// A string literal is a sequence of parts: jq interpolates `\(…)` inside one,
/// so the lexer reports the text and the interpolations separately rather than
/// decoding the literal whole.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum StringPart {
    /// Text between interpolations, with its escapes already decoded.
    Literal(String),
    /// The unparsed source of one `\(…)`.
    Interpolation(String),
}

pub(super) fn lex(query: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = query.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        match character {
            character if character.is_whitespace() => {}
            ':' => tokens.push(Token::Colon),
            ',' => tokens.push(Token::Comma),
            // `..` is one token, but only when the dots touch: `. .` stays two
            // identities, and `.a` still opens a field.
            '.' => {
                let next = chars.peek().copied();
                if matches!(next, Some((position, '.')) if position == index + 1) {
                    chars.next();
                    tokens.push(Token::DotDot);
                } else if matches!(next, Some((position, character))
                    if position == index + 1 && is_ident_start(character))
                {
                    tokens.push(Token::FieldDot);
                } else {
                    tokens.push(Token::Dot);
                }
            }
            '|' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign(AssignOp::Update)
            } else {
                Token::Pipe
            }),
            '+' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign(AssignOp::Arithmetic(BinaryOp::Add))
            } else {
                Token::Plus
            }),
            '%' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign(AssignOp::Arithmetic(BinaryOp::Modulo))
            } else {
                Token::Percent
            }),
            '?' => tokens.push(Token::Question),
            '-' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign(AssignOp::Arithmetic(BinaryOp::Subtract))
            } else {
                Token::Minus
            }),
            '*' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign(AssignOp::Arithmetic(BinaryOp::Multiply))
            } else {
                Token::Star
            }),
            '/' => {
                if consume_char(&mut chars, '/') {
                    tokens.push(if consume_char(&mut chars, '=') {
                        Token::Assign(AssignOp::Alternative)
                    } else {
                        Token::Alternative
                    });
                } else if consume_char(&mut chars, '=') {
                    tokens.push(Token::Assign(AssignOp::Arithmetic(BinaryOp::Divide)));
                } else {
                    tokens.push(Token::Slash);
                }
            }
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            '[' => tokens.push(Token::LBracket),
            ']' => tokens.push(Token::RBracket),
            '{' => tokens.push(Token::LBrace),
            '}' => tokens.push(Token::RBrace),
            ';' => tokens.push(Token::Semicolon),
            '=' => {
                if consume_char(&mut chars, '=') {
                    tokens.push(Token::EqualEqual);
                } else {
                    tokens.push(Token::Assign(AssignOp::Set));
                }
            }
            '!' => {
                expect_char(&mut chars, '=')?;
                tokens.push(Token::NotEqual);
            }
            '<' => {
                if consume_char(&mut chars, '=') {
                    tokens.push(Token::LessEqual);
                } else {
                    tokens.push(Token::Less);
                }
            }
            '>' => {
                if consume_char(&mut chars, '=') {
                    tokens.push(Token::GreaterEqual);
                } else {
                    tokens.push(Token::Greater);
                }
            }
            '$' => {
                let Some((start, character)) = chars.peek().copied() else {
                    return Err("expected variable name after `$`".to_owned());
                };
                if !is_ident_start(character) {
                    return Err("expected variable name after `$`".to_owned());
                }
                let end = read_ident_end(query, start);
                tokens.push(Token::Variable(query[start..end].to_owned()));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            '@' => {
                let Some((start, character)) = chars.peek().copied() else {
                    return Err("expected format name after `@`".to_owned());
                };
                if !is_ident_start(character) {
                    return Err("expected format name after `@`".to_owned());
                }
                let end = read_ident_end(query, start);
                tokens.push(Token::Format(query[start..end].to_owned()));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            '"' => {
                let (parts, end) = read_string(query, index)?;
                tokens.push(Token::String(parts));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            character if character.is_ascii_digit() => {
                let end = read_number_end(query, index);
                tokens.push(Token::Number(query[index..end].to_owned()));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            character if is_ident_start(character) => {
                let end = read_ident_end(query, index);
                tokens.push(Token::Ident(query[index..end].to_owned()));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            _ => return Err(format!("unsupported character `{character}`")),
        }
    }

    Ok(tokens)
}

fn expect_char(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    expected: char,
) -> Result<(), String> {
    if consume_char(chars, expected) {
        Ok(())
    } else {
        Err(format!("expected `{expected}`"))
    }
}

fn consume_char(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    expected: char,
) -> bool {
    if matches!(chars.peek(), Some((_, character)) if *character == expected) {
        chars.next();
        true
    } else {
        false
    }
}

/// Reads the literal opening at `start`, returning its parts and the index one
/// past its closing quote. Only ASCII delimiters end a part, so scanning bytes
/// never splits a multi-byte character.
fn read_string(query: &str, start: usize) -> Result<(Vec<StringPart>, usize), String> {
    let bytes = query.as_bytes();
    let mut parts = Vec::new();
    let mut text = start + 1;
    let mut index = start + 1;

    while index < bytes.len() {
        match bytes[index] {
            // `\(` opens an interpolation; every other escape belongs to the
            // surrounding text and is decoded with it.
            b'\\' if bytes.get(index + 1) == Some(&b'(') => {
                push_literal(&mut parts, &query[text..index])?;
                let (source, end) = read_interpolation(query, index + 1)?;
                parts.push(StringPart::Interpolation(source));
                text = end;
                index = end;
            }
            b'\\' => index += 2,
            b'"' => {
                push_literal(&mut parts, &query[text..index])?;
                return Ok((parts, index + 1));
            }
            _ => index += 1,
        }
    }
    Err("unterminated string literal".to_owned())
}

/// Decodes one run of literal text, which JSON escaping already describes.
fn push_literal(parts: &mut Vec<StringPart>, raw: &str) -> Result<(), String> {
    if raw.is_empty() {
        return Ok(());
    }
    let value = serde_json::from_str(&format!("\"{raw}\""))
        .map_err(|error| format!("invalid string literal: {error}"))?;
    parts.push(StringPart::Literal(value));
    Ok(())
}

/// Reads the source of one interpolation, from its `(` to the matching `)`.
/// Nested strings are read whole so a `)` inside one cannot close it.
fn read_interpolation(query: &str, open: usize) -> Result<(String, usize), String> {
    let bytes = query.as_bytes();
    let mut depth = 1usize;
    let mut index = open + 1;

    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index = read_string(query, index)?.1;
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((query[open + 1..index].to_owned(), index + 1));
                }
            }
            _ => {}
        }
        index += 1;
    }
    Err("unterminated string interpolation".to_owned())
}

fn read_number_end(query: &str, start: usize) -> usize {
    let mut end = start;
    for (index, character) in query[start..].char_indices() {
        if index == 0 || character.is_ascii_digit() || matches!(character, '.' | 'e' | 'E') {
            end = start + index + character.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn read_ident_end(query: &str, start: usize) -> usize {
    let mut end = start;
    for (index, character) in query[start..].char_indices() {
        if index == 0 || is_ident_continue(character) {
            end = start + index + character.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn is_ident_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_ident_continue(character: char) -> bool {
    is_ident_start(character) || character.is_ascii_digit() || character == '-'
}
