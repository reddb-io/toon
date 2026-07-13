use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use reddb_io_toon::{Document, Value};

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
    let mut args = env::args().skip(1);
    let query = args
        .next()
        .ok_or_else(|| "usage: tq <query> [file]".to_owned())?;
    let input_path = args.next();
    if args.next().is_some() {
        return Err("usage: tq <query> [file]".to_owned());
    }

    let input = match input_path {
        Some(path) => fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?,
        None => read_stdin()?,
    };
    let document = Document::parse(&input).map_err(|error| error.to_string())?;

    evaluate(&document, &query)
}

fn read_stdin() -> Result<String, String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("stdin: {error}"))?;
    Ok(input)
}

fn evaluate(document: &Document, query: &str) -> Result<String, String> {
    if query == "." {
        return Ok(document.to_canonical_toon());
    }

    if let Some(path) = query.strip_prefix('.') {
        if path.is_empty() {
            return Err(format!("unsupported query `{query}`"));
        }

        let tokens = parse_path(path)?;
        let values = evaluate_tokens(Value::Object(document.clone()), &tokens);
        return Ok(format_values(&values));
    }

    Err(format!("unsupported query `{query}`"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Field(String),
    Index(usize),
    Slice(Option<usize>, Option<usize>),
    Iter,
}

fn parse_path(path: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut index = 0;
    let bytes = path.as_bytes();

    while index < path.len() {
        if bytes[index] == b'.' {
            index += 1;
            if index == path.len() || bytes[index] == b'.' {
                return Err(format!("unsupported query `.{path}`"));
            }
            continue;
        }

        if bytes[index] == b'[' {
            let close = path[index + 1..]
                .find(']')
                .map(|offset| index + 1 + offset)
                .ok_or_else(|| format!("unsupported query `.{path}`"))?;
            tokens.push(parse_bracket_token(&path[index + 1..close], path)?);
            index = close + 1;
            continue;
        }

        let start = index;
        while index < path.len() && !matches!(bytes[index], b'.' | b'[') {
            index += 1;
        }
        let field = &path[start..index];
        if field.is_empty() {
            return Err(format!("unsupported query `.{path}`"));
        }
        tokens.push(Token::Field(field.to_owned()));
    }

    Ok(tokens)
}

fn parse_bracket_token(token: &str, path: &str) -> Result<Token, String> {
    if token.is_empty() {
        return Ok(Token::Iter);
    }

    if let Some((start, end)) = token.split_once(':') {
        return Ok(Token::Slice(
            parse_optional_index(start, path)?,
            parse_optional_index(end, path)?,
        ));
    }

    token
        .parse()
        .map(Token::Index)
        .map_err(|_| format!("unsupported query `.{path}`"))
}

fn parse_optional_index(value: &str, path: &str) -> Result<Option<usize>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse()
        .map(Some)
        .map_err(|_| format!("unsupported query `.{path}`"))
}

fn evaluate_tokens(root: Value, tokens: &[Token]) -> Vec<Value> {
    let mut values = vec![root];

    for token in tokens {
        values = values
            .into_iter()
            .flat_map(|value| evaluate_token(value, token))
            .collect();
    }

    values
}

fn evaluate_token(value: Value, token: &Token) -> Vec<Value> {
    match token {
        Token::Field(key) => match value {
            Value::Object(document) => vec![document.get(key).cloned().unwrap_or(Value::Null)],
            _ => vec![Value::Null],
        },
        Token::Index(index) => match value {
            Value::Array(array) => vec![array.get(*index).unwrap_or(Value::Null)],
            _ => vec![Value::Null],
        },
        Token::Slice(start, end) => match value {
            Value::Array(array) => vec![Value::Array(array.slice(*start, *end))],
            _ => vec![Value::Null],
        },
        Token::Iter => match value {
            Value::Array(array) => array.values(),
            _ => vec![Value::Null],
        },
    }
}

fn format_values(values: &[Value]) -> String {
    let mut output = String::new();
    for value in values {
        output.push_str(&value.to_canonical_toon());
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    output
}
