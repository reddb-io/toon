use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document<'a> {
    fields: Vec<Field<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<'a> {
    key: &'a str,
    value: Scalar<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    line: usize,
    message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar<'a> {
    Bool(bool),
    Null,
    Number(&'a str),
    String(&'a str),
}

impl<'a> Document<'a> {
    pub fn parse(input: &'a str) -> Result<Self, ParseError> {
        let mut fields = Vec::new();

        for (index, raw_line) in input.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            let (key, value) = line.split_once(':').ok_or(ParseError {
                line: index + 1,
                message: "expected `key: value`",
            })?;
            let key = key.trim();
            if key.is_empty() || key.contains(char::is_whitespace) {
                return Err(ParseError {
                    line: index + 1,
                    message: "expected non-empty field name",
                });
            }

            fields.push(Field {
                key,
                value: parse_scalar(value.trim(), index + 1)?,
            });
        }

        Ok(Self { fields })
    }

    pub fn get(&self, key: &str) -> Option<&Scalar<'a>> {
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| &field.value)
    }

    pub fn to_canonical_toon(&self) -> String {
        let mut output = String::new();
        for field in &self.fields {
            output.push_str(field.key);
            output.push_str(": ");
            output.push_str(&field.value.to_canonical_toon());
            output.push('\n');
        }
        output
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

impl Scalar<'_> {
    pub fn to_canonical_toon(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Null => "null".to_owned(),
            Self::Number(value) | Self::String(value) => (*value).to_owned(),
        }
    }
}

fn parse_scalar(value: &str, line: usize) -> Result<Scalar<'_>, ParseError> {
    if value.is_empty() {
        return Err(ParseError {
            line,
            message: "expected scalar value",
        });
    }

    Ok(match value {
        "true" => Scalar::Bool(true),
        "false" => Scalar::Bool(false),
        "null" => Scalar::Null,
        value if is_number(value) => Scalar::Number(value),
        value => Scalar::String(value),
    })
}

fn is_number(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::Document;

    #[test]
    fn parses_flat_fields_and_serializes_canonical_toon() {
        let document = Document::parse(" name : Ada \nactive: true\ncount: 3\n").unwrap();

        assert_eq!(
            document.to_canonical_toon(),
            "name: Ada\nactive: true\ncount: 3\n"
        );
    }

    #[test]
    fn returns_top_level_scalar_by_name() {
        let document = Document::parse("name: Ada\n").unwrap();

        assert_eq!(
            document.get("name").unwrap().to_canonical_toon(),
            "Ada".to_owned()
        );
    }
}
