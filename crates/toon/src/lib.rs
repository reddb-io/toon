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
    Array(Array),
    Bool(bool),
    Null,
    Number(String),
    Object(Document),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Array {
    List(Vec<Value>),
    Tabular(TabularArray),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabularArray {
    fields: Vec<String>,
    rows: Vec<TabularRow>,
    delimiter: char,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabularRow {
    line: usize,
    values: Vec<String>,
}

#[derive(Debug)]
struct Line<'a> {
    number: usize,
    depth: usize,
    content: &'a str,
}

#[derive(Debug)]
struct ArrayHeader {
    key: String,
    len: usize,
    delimiter: char,
    fields: Option<Vec<String>>,
}

impl Document {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        match Value::parse_toon(input)? {
            Value::Object(document) => Ok(document),
            _ => Err(ParseError {
                line: 1,
                message: "expected `key: value`",
            }),
        }
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

    pub fn to_json_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for field in &self.fields {
            map.insert(field.key.clone(), field.value.to_json_value());
        }
        serde_json::Value::Object(map)
    }

    fn write_canonical_toon(&self, output: &mut String, depth: usize) {
        for field in &self.fields {
            output.push_str(&" ".repeat(depth * INDENT_WIDTH));
            match &field.value {
                Value::Array(array) => {
                    array.write_canonical_toon(output, Some(field.key.as_str()), depth);
                }
                Value::Object(document) => {
                    output.push_str(&canonical_key(&field.key));
                    output.push_str(":\n");
                    document.write_canonical_toon(output, depth + 1);
                }
                value => {
                    output.push_str(&canonical_key(&field.key));
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
    pub fn parse_toon(input: &str) -> Result<Self, ParseError> {
        let lines = collect_lines(input)?;
        let Some(first_line) = lines.first() else {
            return Ok(Self::Object(Document { fields: Vec::new() }));
        };
        if first_line.depth != 0 {
            return Err(ParseError {
                line: first_line.number,
                message: "invalid indentation",
            });
        }

        if let Some(value) = parse_root_array(&lines)? {
            return Ok(value);
        }

        if lines.len() == 1 && try_split_field(first_line.content, first_line.number)?.is_none() {
            let value_text = first_line.content.trim();
            if value_text == "[]" {
                return Ok(Self::Array(Array::List(Vec::new())));
            }
            let value = parse_scalar(value_text, first_line.number)?;
            if matches!(value, Self::String(_)) && !value_text.starts_with('"') {
                return Err(ParseError {
                    line: first_line.number,
                    message: "expected `key: value`",
                });
            }
            return Ok(value);
        }

        let mut index = 0;
        let document = parse_object(&lines, &mut index, 0)?;
        if let Some(line) = lines.get(index) {
            return Err(ParseError {
                line: line.number,
                message: "expected end of document",
            });
        }
        Ok(Self::Object(document))
    }

    pub fn from_json_str(input: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(input).map(Self::from_json_value)
    }

    pub fn from_json_value(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Array(values) => Self::Array(Array::from_json_values(values)),
            serde_json::Value::Bool(value) => Self::Bool(value),
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Number(value) => Self::Number(value.to_string()),
            serde_json::Value::Object(map) => {
                let fields = map
                    .into_iter()
                    .map(|(key, value)| Field {
                        key,
                        value: Self::from_json_value(value),
                    })
                    .collect();
                Self::Object(Document { fields })
            }
            serde_json::Value::String(value) => Self::String(value),
        }
    }

    pub fn to_canonical_toon(&self) -> String {
        match self {
            Self::Array(array) => array.to_canonical_toon(),
            Self::Bool(value) => value.to_string(),
            Self::Null => "null".to_owned(),
            Self::Number(value) => value.clone(),
            Self::Object(document) => document.to_canonical_toon(),
            Self::String(value) => canonical_string(value),
        }
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Self::Array(array) => array.to_json_value(),
            Self::Bool(value) => serde_json::Value::Bool(*value),
            Self::Null => serde_json::Value::Null,
            Self::Number(value) => serde_json::from_str(value)
                .ok()
                .filter(serde_json::Value::is_number)
                .unwrap_or_else(|| serde_json::Value::String(value.clone())),
            Self::Object(document) => document.to_json_value(),
            Self::String(value) => serde_json::Value::String(value.clone()),
        }
    }

    pub fn to_json_string(&self, compact: bool) -> Result<String, serde_json::Error> {
        let value = self.to_json_value();
        if compact {
            serde_json::to_string(&value)
        } else {
            serde_json::to_string_pretty(&value)
        }
    }

    pub fn as_object(&self) -> Option<&Document> {
        match self {
            Self::Object(document) => Some(document),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Array> {
        match self {
            Self::Array(array) => Some(array),
            _ => None,
        }
    }
}

impl Array {
    fn from_json_values(values: Vec<serde_json::Value>) -> Self {
        let values = values
            .into_iter()
            .map(Value::from_json_value)
            .collect::<Vec<_>>();
        try_tabular_json_array(&values).unwrap_or(Self::List(values))
    }

    pub fn len(&self) -> usize {
        match self {
            Self::List(values) => values.len(),
            Self::Tabular(table) => table.rows.len(),
        }
    }

    pub fn get(&self, index: usize) -> Option<Value> {
        match self {
            Self::List(values) => values.get(index).cloned(),
            Self::Tabular(table) => table.get(index),
        }
    }

    pub fn slice(&self, start: Option<usize>, end: Option<usize>) -> Self {
        let len = self.len();
        let start = start.unwrap_or(0).min(len);
        let end = end.unwrap_or(len).min(len);
        let (start, end) = if start > end {
            (end, end)
        } else {
            (start, end)
        };

        match self {
            Self::List(values) => Self::List(values[start..end].to_vec()),
            Self::Tabular(table) => Self::Tabular(TabularArray {
                fields: table.fields.clone(),
                rows: table.rows[start..end].to_vec(),
                delimiter: table.delimiter,
            }),
        }
    }

    pub fn values(&self) -> Vec<Value> {
        (0..self.len())
            .filter_map(|index| self.get(index))
            .collect()
    }

    pub fn to_canonical_toon(&self) -> String {
        let mut output = String::new();
        self.write_canonical_toon(&mut output, None, 0);
        output
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.values()
                .into_iter()
                .map(|value| value.to_json_value())
                .collect(),
        )
    }

    fn write_canonical_toon(&self, output: &mut String, key: Option<&str>, depth: usize) {
        self.write_body(output, key, depth);
    }

    fn write_body(&self, output: &mut String, key: Option<&str>, depth: usize) {
        match self {
            Self::List(values) if values.is_empty() => match key {
                Some(key) => {
                    output.push_str(&canonical_key(key));
                    output.push_str(": []\n");
                }
                None => output.push_str("[]\n"),
            },
            Self::List(values) if values.iter().all(is_inline_array_value) => {
                write_array_header(output, key, values.len(), None);
                output.push(' ');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    output.push_str(&value.to_canonical_toon());
                }
                output.push('\n');
            }
            Self::List(values) => {
                write_array_header(output, key, values.len(), None);
                output.push('\n');
                for value in values {
                    output.push_str(&" ".repeat((depth + 1) * INDENT_WIDTH));
                    match value {
                        Value::Object(document) => {
                            let Some(first) = document.fields.first() else {
                                output.push_str("- null\n");
                                continue;
                            };
                            output.push_str("- ");
                            write_field(output, first, depth + 1);
                            for field in document.fields.iter().skip(1) {
                                output.push_str(&" ".repeat((depth + 2) * INDENT_WIDTH));
                                write_field(output, field, depth + 2);
                            }
                        }
                        value => {
                            output.push_str("- ");
                            output.push_str(&value.to_canonical_toon());
                            output.push('\n');
                        }
                    }
                }
            }
            Self::Tabular(table) => {
                write_array_header(output, key, table.rows.len(), Some(&table.fields));
                output.push('\n');
                for row in &table.rows {
                    output.push_str(&" ".repeat((depth + 1) * INDENT_WIDTH));
                    output.push_str(&table.canonical_row(row));
                    output.push('\n');
                }
            }
        }
    }
}

impl TabularArray {
    fn get(&self, index: usize) -> Option<Value> {
        self.rows
            .get(index)
            .map(|row| Value::Object(self.decode_row(row)))
    }

    fn decode_row(&self, row: &TabularRow) -> Document {
        count_tabular_row_decode_for_tests();
        let fields = self
            .fields
            .iter()
            .zip(&row.values)
            .map(|(key, raw_value)| Field {
                key: key.clone(),
                value: parse_scalar(raw_value, row.line)
                    .unwrap_or_else(|_| Value::String(raw_value.clone())),
            })
            .collect();
        Document { fields }
    }

    fn canonical_row(&self, row: &TabularRow) -> String {
        row.values
            .iter()
            .map(|value| {
                parse_scalar(value, row.line)
                    .unwrap_or_else(|_| Value::String(value.clone()))
                    .to_canonical_toon()
            })
            .collect::<Vec<_>>()
            .join(&self.delimiter.to_string())
    }
}

fn parse_root_array(lines: &[Line<'_>]) -> Result<Option<Value>, ParseError> {
    let first_line = &lines[0];
    let Some((raw_key, raw_value)) = try_split_field(first_line.content, first_line.number)? else {
        return Ok(None);
    };
    if !raw_key.trim_start().starts_with('[') {
        return Ok(None);
    }

    let Some(header) = parse_array_header_with_options(raw_key.trim(), first_line.number, true)?
    else {
        return Ok(None);
    };
    let mut index = 0;
    let value = parse_array_value(
        &header,
        raw_value.trim(),
        lines,
        &mut index,
        0,
        first_line.number,
    )?;
    if let Some(line) = lines.get(index) {
        return Err(ParseError {
            line: line.number,
            message: "expected end of document",
        });
    }
    Ok(Some(value))
}

fn try_tabular_json_array(values: &[Value]) -> Option<Array> {
    let Value::Object(first) = values.first()? else {
        return None;
    };
    if first.fields.is_empty()
        || first
            .fields
            .iter()
            .any(|field| !is_tabular_json_value(&field.value))
    {
        return None;
    }

    let fields = first
        .fields
        .iter()
        .map(|field| field.key.clone())
        .collect::<Vec<_>>();
    let mut rows = Vec::with_capacity(values.len());

    for value in values {
        let Value::Object(document) = value else {
            return None;
        };
        if document.fields.len() != fields.len() {
            return None;
        }

        let mut row_values = Vec::with_capacity(fields.len());
        for (field, expected_key) in document.fields.iter().zip(&fields) {
            if field.key != *expected_key || !is_tabular_json_value(&field.value) {
                return None;
            }
            row_values.push(field.value.to_canonical_toon());
        }
        rows.push(TabularRow {
            line: 0,
            values: row_values,
        });
    }

    Some(Array::Tabular(TabularArray {
        fields,
        rows,
        delimiter: ',',
    }))
}

fn is_tabular_json_value(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
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

        let (key, raw_value) = split_field(line.content, line.number)?;

        let value_text = raw_value.trim();
        if let Some(header) = parse_array_header(key, line.number)? {
            let value = parse_array_value(&header, value_text, lines, index, depth, line.number)?;
            fields.push(Field {
                key: header.key,
                value,
            });
            continue;
        }

        if value_text == "[]" {
            let raw_key = key;
            let key = parse_key(raw_key, line.number)?;
            if key.is_empty() && !raw_key.trim().starts_with('"') {
                return Err(ParseError {
                    line: line.number,
                    message: "expected non-empty field name",
                });
            }
            *index += 1;
            fields.push(Field {
                key,
                value: Value::Array(Array::List(Vec::new())),
            });
            continue;
        }

        if value_text.is_empty() {
            *index += 1;
            match lines.get(*index) {
                Some(next_line) if next_line.depth == depth + 1 => {
                    let raw_key = key;
                    let key = parse_key(raw_key, line.number)?;
                    if key.is_empty() && !raw_key.trim().starts_with('"') {
                        return Err(ParseError {
                            line: line.number,
                            message: "expected non-empty field name",
                        });
                    }
                    let nested = parse_object(lines, index, depth + 1)?;
                    fields.push(Field {
                        key,
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
        let raw_key = key;
        let key = parse_key(raw_key, line.number)?;
        if key.is_empty() && !raw_key.trim().starts_with('"') {
            return Err(ParseError {
                line: line.number,
                message: "expected non-empty field name",
            });
        }
        fields.push(Field { key, value });
    }

    Ok(Document { fields })
}

fn parse_array_header(key: &str, line: usize) -> Result<Option<ArrayHeader>, ParseError> {
    parse_array_header_with_options(key, line, false)
}

fn parse_array_header_with_options(
    key: &str,
    line: usize,
    allow_empty_key: bool,
) -> Result<Option<ArrayHeader>, ParseError> {
    let Some(open) = find_unquoted_char(key, '[', line)? else {
        return Ok(None);
    };
    let Some(close) = key[open + 1..].find(']').map(|offset| open + 1 + offset) else {
        return Err(ParseError {
            line,
            message: "invalid array header",
        });
    };

    let raw_name = &key[..open];
    let name = parse_key(raw_name, line)?;
    if !allow_empty_key && name.is_empty() && !raw_name.trim().starts_with('"') {
        return Err(ParseError {
            line,
            message: "expected non-empty field name",
        });
    }

    let length_and_delimiter = &key[open + 1..close];
    let digit_count = length_and_delimiter
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return Err(ParseError {
            line,
            message: "invalid array header",
        });
    }
    let len = length_and_delimiter[..digit_count]
        .parse()
        .map_err(|_| ParseError {
            line,
            message: "invalid array header",
        })?;
    let delimiter = match &length_and_delimiter[digit_count..] {
        "" => ',',
        text if text.chars().count() == 1 => text.chars().next().unwrap(),
        _ => {
            return Err(ParseError {
                line,
                message: "invalid array header",
            });
        }
    };

    let suffix = &key[close + 1..];
    let fields = if suffix.is_empty() {
        None
    } else if suffix.starts_with('{') && suffix.ends_with('}') {
        let raw_names = split_delimited_values(&suffix[1..suffix.len() - 1], delimiter, line)?;
        let names = raw_names
            .iter()
            .into_iter()
            .map(|name| parse_key(name, line))
            .collect::<Result<Vec<_>, _>>()?;
        if names
            .iter()
            .zip(&raw_names)
            .any(|(name, raw_name)| name.is_empty() && !raw_name.trim().starts_with('"'))
        {
            return Err(ParseError {
                line,
                message: "invalid array header",
            });
        }
        Some(names)
    } else {
        return Err(ParseError {
            line,
            message: "invalid array header",
        });
    };

    Ok(Some(ArrayHeader {
        key: name,
        len,
        delimiter,
        fields,
    }))
}

fn parse_array_value(
    header: &ArrayHeader,
    value_text: &str,
    lines: &[Line<'_>],
    index: &mut usize,
    depth: usize,
    line: usize,
) -> Result<Value, ParseError> {
    if let Some(fields) = &header.fields {
        if !value_text.is_empty() {
            return Err(ParseError {
                line,
                message: "expected tabular rows",
            });
        }
        *index += 1;
        return parse_tabular_array(fields, header, lines, index, depth + 1, line);
    }

    if !value_text.is_empty() {
        let values = split_delimited_values(value_text, header.delimiter, line)?
            .into_iter()
            .map(|value| parse_scalar(&value, line))
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != header.len {
            return Err(ParseError {
                line,
                message: "array length mismatch",
            });
        }
        *index += 1;
        return Ok(Value::Array(Array::List(values)));
    }

    *index += 1;
    parse_expanded_list_array(header, lines, index, depth + 1, line)
}

fn parse_tabular_array(
    fields: &[String],
    header: &ArrayHeader,
    lines: &[Line<'_>],
    index: &mut usize,
    row_depth: usize,
    header_line: usize,
) -> Result<Value, ParseError> {
    let mut rows = Vec::new();

    while rows.len() < header.len {
        let Some(line) = lines.get(*index) else {
            return Err(ParseError {
                line: header_line,
                message: "array length mismatch",
            });
        };
        if line.depth < row_depth {
            return Err(ParseError {
                line: header_line,
                message: "array length mismatch",
            });
        }
        if line.depth != row_depth {
            return Err(ParseError {
                line: line.number,
                message: "invalid indentation",
            });
        }

        let values = split_delimited_values(line.content, header.delimiter, line.number)?;
        if values.len() != fields.len() {
            return Err(ParseError {
                line: line.number,
                message: "array row length mismatch",
            });
        }
        rows.push(TabularRow {
            line: line.number,
            values,
        });
        *index += 1;
    }

    if let Some(line) = lines.get(*index) {
        if line.depth >= row_depth {
            return Err(ParseError {
                line: line.number,
                message: "array length mismatch",
            });
        }
    }

    Ok(Value::Array(Array::Tabular(TabularArray {
        fields: fields.to_vec(),
        rows,
        delimiter: header.delimiter,
    })))
}

fn parse_expanded_list_array(
    header: &ArrayHeader,
    lines: &[Line<'_>],
    index: &mut usize,
    item_depth: usize,
    header_line: usize,
) -> Result<Value, ParseError> {
    let mut values = Vec::new();

    while values.len() < header.len {
        let Some(line) = lines.get(*index) else {
            return Err(ParseError {
                line: header_line,
                message: "array length mismatch",
            });
        };
        if line.depth < item_depth {
            return Err(ParseError {
                line: header_line,
                message: "array length mismatch",
            });
        }
        if line.depth != item_depth {
            return Err(ParseError {
                line: line.number,
                message: "invalid indentation",
            });
        }
        let Some(rest) = line.content.strip_prefix('-') else {
            return Err(ParseError {
                line: line.number,
                message: "expected array item",
            });
        };
        let value_text = rest.trim_start();
        if try_split_field(value_text, line.number)?.is_some() {
            let mut nested_lines = vec![Line {
                number: line.number,
                depth: 0,
                content: value_text,
            }];
            *index += 1;
            while let Some(next_line) = lines.get(*index) {
                if next_line.depth <= item_depth {
                    break;
                }
                nested_lines.push(Line {
                    number: next_line.number,
                    depth: next_line.depth - item_depth - 1,
                    content: next_line.content,
                });
                *index += 1;
            }
            let mut nested_index = 0;
            values.push(Value::Object(parse_object(
                &nested_lines,
                &mut nested_index,
                0,
            )?));
        } else {
            values.push(parse_scalar(value_text, line.number)?);
            *index += 1;
        }
    }

    if let Some(line) = lines.get(*index) {
        if line.depth >= item_depth {
            return Err(ParseError {
                line: line.number,
                message: "array length mismatch",
            });
        }
    }

    Ok(Value::Array(Array::List(values)))
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

fn split_delimited_values(
    value: &str,
    delimiter: char,
    line: usize,
) -> Result<Vec<String>, ParseError> {
    if value.is_empty() {
        return Ok(Vec::new());
    }

    let mut values = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            character if character == delimiter && !in_string => {
                values.push(value[start..index].trim().to_owned());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }

    if in_string || escaped {
        return Err(ParseError {
            line,
            message: "invalid quoted string",
        });
    }

    values.push(value[start..].trim().to_owned());
    Ok(values)
}

fn write_array_header(
    output: &mut String,
    key: Option<&str>,
    len: usize,
    fields: Option<&[String]>,
) {
    if let Some(key) = key {
        output.push_str(&canonical_key(key));
    }
    output.push('[');
    output.push_str(&len.to_string());
    output.push(']');
    if let Some(fields) = fields {
        output.push('{');
        output.push_str(
            &fields
                .iter()
                .map(|field| canonical_key(field))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push('}');
    }
    output.push(':');
}

fn write_field(output: &mut String, field: &Field, depth: usize) {
    match &field.value {
        Value::Array(array) => {
            array.write_canonical_toon(output, Some(&field.key), depth);
        }
        Value::Object(document) => {
            output.push_str(&canonical_key(&field.key));
            output.push_str(":\n");
            document.write_canonical_toon(output, depth + 1);
        }
        value => {
            output.push_str(&canonical_key(&field.key));
            output.push_str(": ");
            output.push_str(&value.to_canonical_toon());
            output.push('\n');
        }
    }
}

fn is_inline_array_value(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

#[cfg(test)]
static TABULAR_ROW_DECODE_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn count_tabular_row_decode_for_tests() {
    #[cfg(test)]
    {
        TABULAR_ROW_DECODE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
fn reset_tabular_row_decode_count_for_tests() {
    TABULAR_ROW_DECODE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
fn tabular_row_decode_count_for_tests() -> usize {
    TABULAR_ROW_DECODE_COUNT.load(std::sync::atomic::Ordering::SeqCst)
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

fn split_field<'a>(content: &'a str, line: usize) -> Result<(&'a str, &'a str), ParseError> {
    try_split_field(content, line)?.ok_or(ParseError {
        line,
        message: "expected `key: value`",
    })
}

fn try_split_field<'a>(
    content: &'a str,
    line: usize,
) -> Result<Option<(&'a str, &'a str)>, ParseError> {
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in content.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            ':' if !in_string => return Ok(Some((&content[..index], &content[index + 1..]))),
            _ => {}
        }
    }

    if in_string || escaped {
        return invalid_quoted_string(line);
    }

    Ok(None)
}

fn find_unquoted_char(value: &str, needle: char, line: usize) -> Result<Option<usize>, ParseError> {
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            character if character == needle && !in_string => return Ok(Some(index)),
            _ => {}
        }
    }

    if in_string || escaped {
        return invalid_quoted_string(line);
    }

    Ok(None)
}

fn parse_key(value: &str, line: usize) -> Result<String, ParseError> {
    let value = value.trim();
    if value.starts_with('"') {
        return parse_quoted_string(value, line);
    }
    if value.is_empty() {
        return Ok(String::new());
    }
    if value.contains('"') || value.contains(char::is_whitespace) {
        return Err(ParseError {
            line,
            message: "expected non-empty field name",
        });
    }
    Ok(value.to_owned())
}

fn canonical_key(value: &str) -> String {
    if can_write_bare_key(value) {
        value.to_owned()
    } else {
        canonical_string(value)
    }
}

fn can_write_bare_key(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
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

    #[test]
    fn parses_inline_list_arrays_and_serializes_canonical_toon() {
        let document = Document::parse("tags[3]: admin,ops,dev\n").unwrap();

        assert_eq!(document.to_canonical_toon(), "tags[3]: admin,ops,dev\n");
    }

    #[test]
    fn parses_tabular_arrays_and_serializes_canonical_toon() {
        let document =
            Document::parse("users[2]{id,name,active}:\n  1,Ada,true\n  2,\"Bob Smith\",false\n")
                .unwrap();

        assert_eq!(
            document.to_canonical_toon(),
            "users[2]{id,name,active}:\n  1,Ada,true\n  2,\"Bob Smith\",false\n"
        );
    }

    #[test]
    fn rejects_array_length_mismatches() {
        let error = Document::parse("tags[2]: admin,ops,dev\n").unwrap_err();

        assert_eq!(error.line(), 1);
        assert_eq!(error.message(), "array length mismatch");
    }

    #[test]
    fn decodes_only_touched_tabular_rows() {
        let document =
            Document::parse("users[3]{id,name}:\n  1,Ada\n  2,Bob\n  3,Chloe\n").unwrap();
        let users = document.get("users").and_then(Value::as_array).unwrap();

        super::reset_tabular_row_decode_count_for_tests();
        let row = users.get(1).unwrap();

        assert_eq!(row.to_canonical_toon(), "id: 2\nname: Bob\n");
        assert_eq!(super::tabular_row_decode_count_for_tests(), 1);
    }
}
