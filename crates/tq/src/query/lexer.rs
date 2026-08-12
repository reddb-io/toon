#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    Alternative,
    Assign,
    Colon,
    Comma,
    Dot,
    EqualEqual,
    Greater,
    GreaterEqual,
    FieldDot,
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
    String(String),
    Variable(String),
}

pub(super) fn lex(query: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = query.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        match character {
            character if character.is_whitespace() => {}
            ':' => tokens.push(Token::Colon),
            ',' => tokens.push(Token::Comma),
            '.' => tokens.push(
                if matches!(chars.peek(), Some((next, character))
                    if *next == index + 1 && is_ident_start(*character))
                {
                    Token::FieldDot
                } else {
                    Token::Dot
                },
            ),
            '|' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign
            } else {
                Token::Pipe
            }),
            '+' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign
            } else {
                Token::Plus
            }),
            '%' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign
            } else {
                Token::Percent
            }),
            '?' => tokens.push(Token::Question),
            '-' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign
            } else {
                Token::Minus
            }),
            '*' => tokens.push(if consume_char(&mut chars, '=') {
                Token::Assign
            } else {
                Token::Star
            }),
            '/' => {
                if consume_char(&mut chars, '/') {
                    tokens.push(if consume_char(&mut chars, '=') {
                        Token::Assign
                    } else {
                        Token::Alternative
                    });
                } else if consume_char(&mut chars, '=') {
                    tokens.push(Token::Assign);
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
                    tokens.push(Token::Assign);
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
            '"' => {
                let (value, end) = read_string(query, index)?;
                tokens.push(Token::String(value));
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

fn read_string(query: &str, start: usize) -> Result<(String, usize), String> {
    let mut escaped = false;
    for (index, character) in query[start + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => {
                let end = start + 1 + index + 1;
                return serde_json::from_str(&query[start..end])
                    .map(|value| (value, end))
                    .map_err(|error| format!("invalid string literal: {error}"));
            }
            _ => {}
        }
    }
    Err("unterminated string literal".to_owned())
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
