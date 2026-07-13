use std::fmt;

const INDENT_WIDTH: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    key: String,
    value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    line: usize,
    message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Null,
    Number(String),
    Object(Document),
    String(String),
}

#[derive(Debug)]
struct Line<'a> {
    number: usize,
    depth: usize,
    content: &'a str,
}

impl Document {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let lines = collect_lines(input)?;
        let mut index = 0;
        parse_object(&lines, &mut index, 0)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| &field.value)
    }

    pub fn to_canonical_toon(&self) -> String {
        let mut output = String::new();
        self.write_canonical_toon(&mut output, 0);
        output
    }

    fn write_canonical_toon(&self, output: &mut String, depth: usize) {
        for field in &self.fields {
            output.push_str(&" ".repeat(depth * INDENT_WIDTH));
            output.push_str(&field.key);
            match &field.value {
                Value::Object(document) => {
                    output.push_str(":\n");
                    document.write_canonical_toon(output, depth + 1);
                }
                value => {
                    output.push_str(": ");
                    output.push_str(&value.to_canonical_toon());
                    output.push('\n');
                }
            }
        }
    }
}

impl ParseError {
    pub fn line(&self) -> usize {
        self.line
    }

    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

impl Value {
    pub fn to_canonical_toon(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Null => "null".to_owned(),
            Self::Number(value) => value.clone(),
            Self::Object(document) => document.to_canonical_toon(),
            Self::String(value) => canonical_string(value),
        }
    }

    pub fn as_object(&self) -> Option<&Document> {
        match self {
            Self::Object(document) => Some(document),
            _ => None,
        }
    }
}

fn collect_lines(input: &str) -> Result<Vec<Line<'_>>, ParseError> {
    let mut lines = Vec::new();

    for (index, raw_line) in input.lines().enumerate() {
        let line_number = index + 1;
        let raw_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if raw_line.trim().is_empty() {
            continue;
        }

        let mut spaces = 0;
        for character in raw_line.chars() {
            match character {
                ' ' => spaces += 1,
                '\t' => {
                    return Err(ParseError {
                        line: line_number,
                        message: "invalid indentation",
                    });
                }
                _ => break,
            }
        }

        if spaces % INDENT_WIDTH != 0 {
            return Err(ParseError {
                line: line_number,
                message: "invalid indentation",
            });
        }

        lines.push(Line {
            number: line_number,
            depth: spaces / INDENT_WIDTH,
            content: &raw_line[spaces..],
        });
    }

    Ok(lines)
}

fn parse_object(
    lines: &[Line<'_>],
    index: &mut usize,
    depth: usize,
) -> Result<Document, ParseError> {
    let mut fields = Vec::new();

    while let Some(line) = lines.get(*index) {
        if line.depth < depth {
            break;
        }
        if line.depth > depth {
            return Err(ParseError {
                line: line.number,
                message: "invalid indentation",
            });
        }

        let (key, raw_value) = line.content.split_once(':').ok_or(ParseError {
            line: line.number,
            message: "expected `key: value`",
        })?;
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return Err(ParseError {
                line: line.number,
                message: "expected non-empty field name",
            });
        }

        let value_text = raw_value.trim();
        if value_text.is_empty() {
            *index += 1;
            match lines.get(*index) {
                Some(next_line) if next_line.depth == depth + 1 => {
                    let nested = parse_object(lines, index, depth + 1)?;
                    fields.push(Field {
                        key: key.to_owned(),
                        value: Value::Object(nested),
                    });
                }
                Some(next_line) if next_line.depth > depth + 1 => {
                    return Err(ParseError {
                        line: next_line.number,
                        message: "invalid indentation",
                    });
                }
                _ => {
                    return Err(ParseError {
                        line: line.number,
                        message: "expected nested fields",
                    });
                }
            }
            continue;
        }

        let value = parse_scalar(value_text, line.number)?;
        *index += 1;
        if let Some(next_line) = lines.get(*index) {
            if next_line.depth > depth {
                return Err(ParseError {
                    line: next_line.number,
                    message: "invalid indentation",
                });
            }
        }
        fields.push(Field {
            key: key.to_owned(),
            value,
        });
    }

    Ok(Document { fields })
}

fn parse_scalar(value: &str, line: usize) -> Result<Value, ParseError> {
    if value.is_empty() {
        return Err(ParseError {
            line,
            message: "expected scalar value",
        });
    }

    if value.starts_with('"') {
        return parse_quoted_string(value, line).map(Value::String);
    }

    if value.contains('"') {
        return Err(ParseError {
            line,
            message: "invalid quoted string",
        });
    }

    Ok(match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        value if is_number(value) => Value::Number(value.to_owned()),
        value => Value::String(value.to_owned()),
    })
}

fn parse_quoted_string(value: &str, line: usize) -> Result<String, ParseError> {
    let mut characters = value.chars();
    if characters.next() != Some('"') {
        return invalid_quoted_string(line);
    }

    let mut output = String::new();
    while let Some(character) = characters.next() {
        match character {
            '"' => {
                if characters.as_str().trim().is_empty() {
                    return Ok(output);
                }
                return invalid_quoted_string(line);
            }
            '\\' => {
                let escaped = characters.next().ok_or(ParseError {
                    line,
                    message: "invalid quoted string",
                })?;
                match escaped {
                    '"' => output.push('"'),
                    '\\' => output.push('\\'),
                    '/' => output.push('/'),
                    'b' => output.push('\u{0008}'),
                    'f' => output.push('\u{000c}'),
                    'n' => output.push('\n'),
                    'r' => output.push('\r'),
                    't' => output.push('\t'),
                    'u' => output.push(parse_unicode_escape(&mut characters, line)?),
                    _ => return invalid_quoted_string(line),
                }
            }
            character if character.is_control() => return invalid_quoted_string(line),
            character => output.push(character),
        }
    }

    invalid_quoted_string(line)
}

fn parse_unicode_escape(
    characters: &mut std::str::Chars<'_>,
    line: usize,
) -> Result<char, ParseError> {
    let mut value = 0;
    for _ in 0..4 {
        let character = characters.next().ok_or(ParseError {
            line,
            message: "invalid quoted string",
        })?;
        value = value * 16
            + character.to_digit(16).ok_or(ParseError {
                line,
                message: "invalid quoted string",
            })?;
    }

    char::from_u32(value).ok_or(ParseError {
        line,
        message: "invalid quoted string",
    })
}

fn invalid_quoted_string<T>(line: usize) -> Result<T, ParseError> {
    Err(ParseError {
        line,
        message: "invalid quoted string",
    })
}

fn canonical_string(value: &str) -> String {
    if can_write_bare_string(value) {
        return value.to_owned();
    }

    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn can_write_bare_string(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "true" | "false" | "null")
        && !is_number(value)
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

fn is_number(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;

    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }

    if !consume_digits(bytes, &mut index) {
        return false;
    }

    if bytes.get(index) == Some(&b'.') {
        index += 1;
        if !consume_digits(bytes, &mut index) {
            return false;
        }
    }

    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        if !consume_digits(bytes, &mut index) {
            return false;
        }
    }

    index == bytes.len()
}

fn consume_digits(bytes: &[u8], index: &mut usize) -> bool {
    let start = *index;
    while matches!(bytes.get(*index), Some(b'0'..=b'9')) {
        *index += 1;
    }
    *index > start
}

#[cfg(test)]
mod tests {
    use super::{Document, Value};

    #[test]
    fn parses_flat_fields_and_serializes_canonical_toon() {
        let document = Document::parse("name : Ada\nactive: true\ncount: 3\n").unwrap();

        assert_eq!(
            document.to_canonical_toon(),
            "name: Ada\nactive: true\ncount: 3\n"
        );
    }

    #[test]
    fn returns_top_level_value_by_name() {
        let document = Document::parse("name: Ada\n").unwrap();

        assert_eq!(
            document.get("name").unwrap().to_canonical_toon(),
            "Ada".to_owned()
        );
    }

    #[test]
    fn parses_nested_objects() {
        let document = Document::parse("person:\n  address:\n    city: London\n").unwrap();

        let person = document.get("person").and_then(Value::as_object).unwrap();
        let address = person.get("address").and_then(Value::as_object).unwrap();

        assert_eq!(
            address.get("city").unwrap().to_canonical_toon(),
            "London".to_owned()
        );
    }

    #[test]
    fn rejects_scalar_children() {
        let error = Document::parse("person: Ada\n  city: London\n").unwrap_err();

        assert_eq!(error.line(), 2);
        assert_eq!(error.message(), "invalid indentation");
    }
}
