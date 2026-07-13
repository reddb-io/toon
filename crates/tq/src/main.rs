use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use reddb_io_toon::Value;

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
        Some(path) => fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?,
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
    if query == "." {
        return Ok(vec![document.clone()]);
    }

    if let Some(path) = query.strip_prefix('.') {
        if path.is_empty() {
            return Err(format!("unsupported query `{query}`"));
        }

        let tokens = parse_path(path)?;
        return Ok(evaluate_tokens(document.clone(), &tokens));
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
