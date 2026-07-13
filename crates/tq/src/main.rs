use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use reddb_io_toon::{Array, Value};

const USAGE: &str = "usage: tq [-p toon|json] [-o toon|json] [-r] [-c] <query> [file]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Toon,
}

#[derive(Debug)]
struct Options {
    query: String,
    input_path: Option<String>,
    input_format: Format,
    output_format: Format,
    raw_output: bool,
    compact: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let options = parse_args(env::args().skip(1))?;

    let input = match &options.input_path {
        Some(path) => fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?,
        None => read_stdin()?,
    };
    let document = match options.input_format {
        Format::Json => Value::from_json_str(&input).map_err(|error| error.to_string())?,
        Format::Toon => Value::parse_toon(&input).map_err(|error| error.to_string())?,
    };

    let values = evaluate(&document, &options.query)?;
    format_values(&values, &options)
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut input_format = Format::Toon;
    let mut output_format = None;
    let mut raw_output = false;
    let mut compact = false;
    let mut positional = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => {
                let format = args.next().ok_or_else(|| USAGE.to_owned())?;
                input_format = parse_format(&format)?;
            }
            "-o" => {
                let format = args.next().ok_or_else(|| USAGE.to_owned())?;
                output_format = Some(parse_format(&format)?);
            }
            "-r" => raw_output = true,
            "-c" => compact = true,
            "--" => {
                positional.extend(args);
                break;
            }
            value if value.starts_with('-') => return Err(USAGE.to_owned()),
            value => positional.push(value.to_owned()),
        }
    }

    if positional.is_empty() || positional.len() > 2 {
        return Err(USAGE.to_owned());
    }

    Ok(Options {
        query: positional.remove(0),
        input_path: positional.pop(),
        input_format,
        output_format: output_format.unwrap_or(input_format),
        raw_output,
        compact,
    })
}

fn parse_format(value: &str) -> Result<Format, String> {
    match value {
        "json" => Ok(Format::Json),
        "toon" => Ok(Format::Toon),
        _ => Err(format!("unsupported format `{value}`")),
    }
}

fn read_stdin() -> Result<String, String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("stdin: {error}"))?;
    Ok(input)
}

fn evaluate(document: &Value, query: &str) -> Result<Vec<Value>, String> {
    Parser::new(query)?.parse()?.eval(document)
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Array(Vec<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Builtin(Builtin),
    Comma(Vec<Expr>),
    Field(Box<Expr>, String),
    Identity,
    Index(Box<Expr>, usize),
    Iter(Box<Expr>),
    Literal(Value),
    Object(Vec<(String, Expr)>),
    Pipe(Box<Expr>, Box<Expr>),
    Slice(Box<Expr>, Option<usize>, Option<usize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, PartialEq)]
enum Builtin {
    Has(Box<Expr>),
    Keys,
    Length,
    Map(Box<Expr>),
    Select(Box<Expr>),
}

impl Expr {
    fn eval(&self, input: &Value) -> Result<Vec<Value>, String> {
        match self {
            Self::Array(items) => {
                let mut values = Vec::new();
                for item in items {
                    values.extend(item.eval(input)?);
                }
                Ok(vec![Value::Array(Array::List(values))])
            }
            Self::Binary(operator, left, right) => {
                let left_values = left.eval(input)?;
                let right_values = right.eval(input)?;
                let mut output = Vec::new();
                for left_value in &left_values {
                    for right_value in &right_values {
                        output.push(evaluate_binary(*operator, left_value, right_value)?);
                    }
                }
                Ok(output)
            }
            Self::Builtin(builtin) => evaluate_builtin(builtin, input),
            Self::Comma(expressions) => {
                let mut output = Vec::new();
                for expression in expressions {
                    output.extend(expression.eval(input)?);
                }
                Ok(output)
            }
            Self::Field(base, key) => Ok(base
                .eval(input)?
                .into_iter()
                .map(|value| match value {
                    Value::Object(document) => document.get(key).cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
                .collect()),
            Self::Identity => Ok(vec![input.clone()]),
            Self::Index(base, index) => Ok(base
                .eval(input)?
                .into_iter()
                .map(|value| match value {
                    Value::Array(array) => array.get(*index).unwrap_or(Value::Null),
                    _ => Value::Null,
                })
                .collect()),
            Self::Iter(base) => Ok(base
                .eval(input)?
                .into_iter()
                .flat_map(|value| match value {
                    Value::Array(array) => array.values(),
                    _ => vec![Value::Null],
                })
                .collect()),
            Self::Literal(value) => Ok(vec![value.clone()]),
            Self::Object(fields) => evaluate_object(fields, input),
            Self::Pipe(left, right) => {
                let mut output = Vec::new();
                for value in left.eval(input)? {
                    output.extend(right.eval(&value)?);
                }
                Ok(output)
            }
            Self::Slice(base, start, end) => Ok(base
                .eval(input)?
                .into_iter()
                .map(|value| match value {
                    Value::Array(array) => Value::Array(array.slice(*start, *end)),
                    _ => Value::Null,
                })
                .collect()),
        }
    }
}

fn evaluate_object(fields: &[(String, Expr)], input: &Value) -> Result<Vec<Value>, String> {
    let mut objects = vec![serde_json::Map::new()];
    for (key, expression) in fields {
        let values = expression.eval(input)?;
        let mut next_objects = Vec::new();
        for object in objects {
            for value in &values {
                let mut next = object.clone();
                next.insert(key.clone(), value.to_json_value());
                next_objects.push(next);
            }
        }
        objects = next_objects;
    }

    Ok(objects
        .into_iter()
        .map(serde_json::Value::Object)
        .map(Value::from_json_value)
        .collect())
}

fn evaluate_builtin(builtin: &Builtin, input: &Value) -> Result<Vec<Value>, String> {
    match builtin {
        Builtin::Has(key_filter) => {
            let keys = key_filter.eval(input)?;
            keys.into_iter()
                .map(|key| evaluate_has(input, &key))
                .collect()
        }
        Builtin::Keys => evaluate_keys(input).map(|value| vec![value]),
        Builtin::Length => evaluate_length(input).map(|value| vec![value]),
        Builtin::Map(filter) => {
            let Value::Array(array) = input else {
                return Err("cannot iterate over non-array".to_owned());
            };
            let mut values = Vec::new();
            for value in array.values() {
                values.extend(filter.eval(&value)?);
            }
            Ok(vec![Value::Array(Array::List(values))])
        }
        Builtin::Select(filter) => {
            let mut values = Vec::new();
            for value in filter.eval(input)? {
                if is_truthy(&value) {
                    values.push(input.clone());
                }
            }
            Ok(values)
        }
    }
}

fn evaluate_has(input: &Value, key: &Value) -> Result<Value, String> {
    match (input, key) {
        (Value::Array(array), Value::Number(index)) => {
            let index = parse_usize(index)?;
            Ok(Value::Bool(index < array.len()))
        }
        (Value::Object(document), Value::String(key)) => {
            Ok(Value::Bool(document.get(key).is_some()))
        }
        (Value::Object(document), Value::Number(key)) => {
            Ok(Value::Bool(document.get(key).is_some()))
        }
        _ => Err("has() cannot check this value".to_owned()),
    }
}

fn evaluate_keys(input: &Value) -> Result<Value, String> {
    match input {
        Value::Array(array) => Ok(Value::Array(Array::List(
            (0..array.len())
                .map(|index| Value::Number(index.to_string()))
                .collect(),
        ))),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            Ok(Value::Array(Array::List(
                keys.into_iter().map(Value::String).collect(),
            )))
        }
        _ => Err("keys cannot be applied to this value".to_owned()),
    }
}

fn evaluate_length(input: &Value) -> Result<Value, String> {
    let length = match input {
        Value::Array(array) => array.len() as f64,
        Value::Null => 0.0,
        Value::Number(value) => parse_number(value)?.abs(),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            map.len() as f64
        }
        Value::String(value) => value.chars().count() as f64,
        Value::Bool(_) => return Err("boolean has no length".to_owned()),
    };
    number_value(length)
}

fn evaluate_binary(operator: BinaryOp, left: &Value, right: &Value) -> Result<Value, String> {
    match operator {
        BinaryOp::Add => add_values(left, right),
        BinaryOp::Subtract => subtract_values(left, right),
        BinaryOp::Multiply => number_value(parse_number_value(left)? * parse_number_value(right)?),
        BinaryOp::Divide => {
            let divisor = parse_number_value(right)?;
            if divisor == 0.0 {
                return Err("division by zero".to_owned());
            }
            number_value(parse_number_value(left)? / divisor)
        }
        BinaryOp::Equal => Ok(Value::Bool(left.to_json_value() == right.to_json_value())),
        BinaryOp::NotEqual => Ok(Value::Bool(left.to_json_value() != right.to_json_value())),
        BinaryOp::Less => Ok(Value::Bool(compare_values(left, right)?.is_lt())),
        BinaryOp::LessEqual => Ok(Value::Bool(!compare_values(left, right)?.is_gt())),
        BinaryOp::Greater => Ok(Value::Bool(compare_values(left, right)?.is_gt())),
        BinaryOp::GreaterEqual => Ok(Value::Bool(!compare_values(left, right)?.is_lt())),
    }
}

fn add_values(left: &Value, right: &Value) -> Result<Value, String> {
    match (left, right) {
        (Value::Null, value) | (value, Value::Null) => Ok(value.clone()),
        (Value::Number(left), Value::Number(right)) => {
            number_value(parse_number(left)? + parse_number(right)?)
        }
        (Value::String(left), Value::String(right)) => Ok(Value::String(format!("{left}{right}"))),
        (Value::Array(left), Value::Array(right)) => {
            let mut values = left.values();
            values.extend(right.values());
            Ok(Value::Array(Array::List(values)))
        }
        (Value::Object(_), Value::Object(_)) => {
            let serde_json::Value::Object(mut left) = left.to_json_value() else {
                unreachable!("object serializes as object");
            };
            let serde_json::Value::Object(right) = right.to_json_value() else {
                unreachable!("object serializes as object");
            };
            left.extend(right);
            Ok(Value::from_json_value(serde_json::Value::Object(left)))
        }
        _ => Err("cannot add these values".to_owned()),
    }
}

fn subtract_values(left: &Value, right: &Value) -> Result<Value, String> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            number_value(parse_number(left)? - parse_number(right)?)
        }
        (Value::Array(left), Value::Array(right)) => {
            let remove = right
                .values()
                .into_iter()
                .map(|value| value.to_json_value())
                .collect::<Vec<_>>();
            Ok(Value::Array(Array::List(
                left.values()
                    .into_iter()
                    .filter(|value| !remove.contains(&value.to_json_value()))
                    .collect(),
            )))
        }
        _ => Err("cannot subtract these values".to_owned()),
    }
}

fn compare_values(left: &Value, right: &Value) -> Result<std::cmp::Ordering, String> {
    let left_json = left.to_json_value();
    let right_json = right.to_json_value();
    compare_json_values(&left_json, &right_json)
}

fn compare_json_values(
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> Result<std::cmp::Ordering, String> {
    let left_rank = json_rank(left);
    let right_rank = json_rank(right);
    if left_rank != right_rank {
        return Ok(left_rank.cmp(&right_rank));
    }

    match (left, right) {
        (serde_json::Value::Null, serde_json::Value::Null) => Ok(std::cmp::Ordering::Equal),
        (serde_json::Value::Bool(left), serde_json::Value::Bool(right)) => Ok(left.cmp(right)),
        (serde_json::Value::Number(left), serde_json::Value::Number(right)) => {
            parse_number(&left.to_string())?
                .partial_cmp(&parse_number(&right.to_string())?)
                .ok_or_else(|| "cannot compare numbers".to_owned())
        }
        (serde_json::Value::String(left), serde_json::Value::String(right)) => Ok(left.cmp(right)),
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            for (left, right) in left.iter().zip(right) {
                let ordering = compare_json_values(left, right)?;
                if !ordering.is_eq() {
                    return Ok(ordering);
                }
            }
            Ok(left.len().cmp(&right.len()))
        }
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            let mut left_entries = left.iter().collect::<Vec<_>>();
            let mut right_entries = right.iter().collect::<Vec<_>>();
            left_entries.sort_by_key(|(key, _)| *key);
            right_entries.sort_by_key(|(key, _)| *key);
            for ((left_key, left_value), (right_key, right_value)) in
                left_entries.iter().zip(&right_entries)
            {
                let key_ordering = left_key.cmp(right_key);
                if !key_ordering.is_eq() {
                    return Ok(key_ordering);
                }
                let value_ordering = compare_json_values(left_value, right_value)?;
                if !value_ordering.is_eq() {
                    return Ok(value_ordering);
                }
            }
            Ok(left_entries.len().cmp(&right_entries.len()))
        }
        _ => unreachable!("matching ranks have matching JSON variants"),
    }
}

fn json_rank(value: &serde_json::Value) -> u8 {
    match value {
        serde_json::Value::Null => 0,
        serde_json::Value::Bool(false) => 1,
        serde_json::Value::Bool(true) => 2,
        serde_json::Value::Number(_) => 3,
        serde_json::Value::String(_) => 4,
        serde_json::Value::Array(_) => 5,
        serde_json::Value::Object(_) => 6,
    }
}

fn is_truthy(value: &Value) -> bool {
    !matches!(value, Value::Bool(false) | Value::Null)
}

fn parse_number_value(value: &Value) -> Result<f64, String> {
    match value {
        Value::Number(value) => parse_number(value),
        _ => Err("expected number".to_owned()),
    }
}

fn parse_number(value: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|_| format!("invalid number `{value}`"))
}

fn parse_usize(value: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("invalid array index `{value}`"))
}

fn number_value(value: f64) -> Result<Value, String> {
    if !value.is_finite() {
        return Err("number is not finite".to_owned());
    }
    if value.fract() == 0.0 {
        Ok(Value::Number(format!("{value:.0}")))
    } else {
        serde_json::Number::from_f64(value)
            .map(|number| Value::Number(number.to_string()))
            .ok_or_else(|| "number is not finite".to_owned())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum LexToken {
    Colon,
    Comma,
    Dot,
    EqualEqual,
    Greater,
    GreaterEqual,
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
    RBrace,
    RBracket,
    RParen,
    Slash,
    Star,
    String(String),
}

struct Parser {
    tokens: Vec<LexToken>,
    index: usize,
}

impl Parser {
    fn new(query: &str) -> Result<Self, String> {
        Ok(Self {
            tokens: lex(query)?,
            index: 0,
        })
    }

    fn parse(mut self) -> Result<Expr, String> {
        let expression = self.parse_pipe()?;
        if self.peek().is_some() {
            return Err("unexpected trailing filter input".to_owned());
        }
        Ok(expression)
    }

    fn parse_pipe(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_comma()?;
        while self.consume(&LexToken::Pipe) {
            let right = self.parse_comma()?;
            expression = Expr::Pipe(Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_comma(&mut self) -> Result<Expr, String> {
        let mut expressions = vec![self.parse_comparison()?];
        while self.consume(&LexToken::Comma) {
            expressions.push(self.parse_comparison()?);
        }
        if expressions.len() == 1 {
            Ok(expressions.pop().expect("one expression exists"))
        } else {
            Ok(Expr::Comma(expressions))
        }
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_additive()?;
        while let Some(operator) = self.match_comparison_operator() {
            let right = self.parse_additive()?;
            expression = Expr::Binary(operator, Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_multiplicative()?;
        loop {
            let operator = if self.consume(&LexToken::Plus) {
                BinaryOp::Add
            } else if self.consume(&LexToken::Minus) {
                BinaryOp::Subtract
            } else {
                break;
            };
            let right = self.parse_multiplicative()?;
            expression = Expr::Binary(operator, Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_unary()?;
        loop {
            let operator = if self.consume(&LexToken::Star) {
                BinaryOp::Multiply
            } else if self.consume(&LexToken::Slash) {
                BinaryOp::Divide
            } else {
                break;
            };
            let right = self.parse_unary()?;
            expression = Expr::Binary(operator, Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.consume(&LexToken::Minus) {
            let expression = self.parse_unary()?;
            return Ok(Expr::Binary(
                BinaryOp::Subtract,
                Box::new(Expr::Literal(Value::Number("0".to_owned()))),
                Box::new(expression),
            ));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_primary()?;
        loop {
            if self.consume(&LexToken::Dot) {
                let key = self.expect_ident()?;
                expression = Expr::Field(Box::new(expression), key);
                continue;
            }

            if self.consume(&LexToken::LBracket) {
                if self.consume(&LexToken::RBracket) {
                    expression = Expr::Iter(Box::new(expression));
                    continue;
                }

                let start = if self.peek() == Some(&LexToken::Colon) {
                    None
                } else {
                    Some(self.expect_usize()?)
                };
                if self.consume(&LexToken::Colon) {
                    let end = if self.peek() == Some(&LexToken::RBracket) {
                        None
                    } else {
                        Some(self.expect_usize()?)
                    };
                    self.expect(LexToken::RBracket)?;
                    expression = Expr::Slice(Box::new(expression), start, end);
                } else {
                    let index = start.ok_or_else(|| "expected array index".to_owned())?;
                    self.expect(LexToken::RBracket)?;
                    expression = Expr::Index(Box::new(expression), index);
                }
                continue;
            }

            break;
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(LexToken::Dot) => {
                let mut expression = Expr::Identity;
                if matches!(self.peek(), Some(LexToken::Ident(_))) {
                    let key = self.expect_ident()?;
                    expression = Expr::Field(Box::new(expression), key);
                }
                Ok(expression)
            }
            Some(LexToken::Ident(value)) => self.parse_identifier(value),
            Some(LexToken::LBracket) => self.parse_array_constructor(),
            Some(LexToken::LBrace) => self.parse_object_constructor(),
            Some(LexToken::LParen) => {
                let expression = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(expression)
            }
            Some(LexToken::Number(value)) => Ok(Expr::Literal(Value::Number(value))),
            Some(LexToken::String(value)) => Ok(Expr::Literal(Value::String(value))),
            token => Err(format!("unexpected token `{token:?}`")),
        }
    }

    fn parse_identifier(&mut self, value: String) -> Result<Expr, String> {
        match value.as_str() {
            "false" => Ok(Expr::Literal(Value::Bool(false))),
            "has" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Has(Box::new(filter))))
            }
            "keys" => Ok(Expr::Builtin(Builtin::Keys)),
            "length" => Ok(Expr::Builtin(Builtin::Length)),
            "map" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Map(Box::new(filter))))
            }
            "null" => Ok(Expr::Literal(Value::Null)),
            "select" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Select(Box::new(filter))))
            }
            "true" => Ok(Expr::Literal(Value::Bool(true))),
            _ => Err(format!("unsupported identifier `{value}`")),
        }
    }

    fn parse_array_constructor(&mut self) -> Result<Expr, String> {
        if self.consume(&LexToken::RBracket) {
            return Ok(Expr::Array(Vec::new()));
        }

        let mut items = Vec::new();
        loop {
            items.push(self.parse_pipe_item()?);
            if self.consume(&LexToken::Comma) {
                continue;
            }
            self.expect(LexToken::RBracket)?;
            break;
        }
        Ok(Expr::Array(items))
    }

    fn parse_object_constructor(&mut self) -> Result<Expr, String> {
        if self.consume(&LexToken::RBrace) {
            return Ok(Expr::Object(Vec::new()));
        }

        let mut fields = Vec::new();
        loop {
            let key = match self.next() {
                Some(LexToken::Ident(value)) | Some(LexToken::String(value)) => value,
                token => return Err(format!("expected object key, got `{token:?}`")),
            };
            self.expect(LexToken::Colon)?;
            fields.push((key, self.parse_pipe_item()?));
            if self.consume(&LexToken::Comma) {
                continue;
            }
            self.expect(LexToken::RBrace)?;
            break;
        }
        Ok(Expr::Object(fields))
    }

    fn parse_pipe_item(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_comparison()?;
        while self.consume(&LexToken::Pipe) {
            let right = self.parse_comparison()?;
            expression = Expr::Pipe(Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn match_comparison_operator(&mut self) -> Option<BinaryOp> {
        let operator = match self.peek()? {
            LexToken::EqualEqual => BinaryOp::Equal,
            LexToken::Greater => BinaryOp::Greater,
            LexToken::GreaterEqual => BinaryOp::GreaterEqual,
            LexToken::Less => BinaryOp::Less,
            LexToken::LessEqual => BinaryOp::LessEqual,
            LexToken::NotEqual => BinaryOp::NotEqual,
            _ => return None,
        };
        self.index += 1;
        Some(operator)
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.next() {
            Some(LexToken::Ident(value)) => Ok(value),
            token => Err(format!("expected identifier, got `{token:?}`")),
        }
    }

    fn expect_usize(&mut self) -> Result<usize, String> {
        match self.next() {
            Some(LexToken::Number(value)) => parse_usize(&value),
            token => Err(format!("expected array index, got `{token:?}`")),
        }
    }

    fn expect(&mut self, expected: LexToken) -> Result<(), String> {
        let actual = self.next();
        if actual == Some(expected.clone()) {
            Ok(())
        } else {
            Err(format!("expected `{expected:?}`, got `{actual:?}`"))
        }
    }

    fn consume(&mut self, expected: &LexToken) -> bool {
        if self.peek() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn next(&mut self) -> Option<LexToken> {
        let token = self.tokens.get(self.index).cloned()?;
        self.index += 1;
        Some(token)
    }

    fn peek(&self) -> Option<&LexToken> {
        self.tokens.get(self.index)
    }
}

fn lex(query: &str) -> Result<Vec<LexToken>, String> {
    let mut tokens = Vec::new();
    let mut chars = query.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        match character {
            character if character.is_whitespace() => {}
            ':' => tokens.push(LexToken::Colon),
            ',' => tokens.push(LexToken::Comma),
            '.' => tokens.push(LexToken::Dot),
            '|' => tokens.push(LexToken::Pipe),
            '+' => tokens.push(LexToken::Plus),
            '-' => tokens.push(LexToken::Minus),
            '*' => tokens.push(LexToken::Star),
            '/' => tokens.push(LexToken::Slash),
            '(' => tokens.push(LexToken::LParen),
            ')' => tokens.push(LexToken::RParen),
            '[' => tokens.push(LexToken::LBracket),
            ']' => tokens.push(LexToken::RBracket),
            '{' => tokens.push(LexToken::LBrace),
            '}' => tokens.push(LexToken::RBrace),
            '=' => {
                expect_char(&mut chars, '=')?;
                tokens.push(LexToken::EqualEqual);
            }
            '!' => {
                expect_char(&mut chars, '=')?;
                tokens.push(LexToken::NotEqual);
            }
            '<' => {
                if consume_char(&mut chars, '=') {
                    tokens.push(LexToken::LessEqual);
                } else {
                    tokens.push(LexToken::Less);
                }
            }
            '>' => {
                if consume_char(&mut chars, '=') {
                    tokens.push(LexToken::GreaterEqual);
                } else {
                    tokens.push(LexToken::Greater);
                }
            }
            '"' => {
                let (value, end) = read_string(query, index)?;
                tokens.push(LexToken::String(value));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            character if character.is_ascii_digit() => {
                let end = read_number_end(query, index);
                tokens.push(LexToken::Number(query[index..end].to_owned()));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            character if is_ident_start(character) => {
                let end = read_ident_end(query, index);
                tokens.push(LexToken::Ident(query[index..end].to_owned()));
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

fn format_values(values: &[Value], options: &Options) -> Result<String, String> {
    let mut output = String::new();
    for value in values {
        if options.raw_output {
            if let Value::String(value) = value {
                output.push_str(value);
                output.push('\n');
                continue;
            }
        }

        match options.output_format {
            Format::Json => {
                output.push_str(
                    &value
                        .to_json_string(options.compact)
                        .map_err(|error| error.to_string())?,
                );
                output.push('\n');
            }
            Format::Toon => {
                output.push_str(&value.to_canonical_toon());
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }
    Ok(output)
}
