use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use reddb_io_toon::Document;

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
        if path.is_empty() || path.split('.').any(str::is_empty) {
            return Err(format!("unsupported query `{query}`"));
        }

        let mut components = path.split('.').peekable();
        let mut current = document;
        while let Some(component) = components.next() {
            let Some(value) = current.get(component) else {
                return Ok("null\n".to_owned());
            };

            if components.peek().is_none() {
                return Ok(format!("{}\n", value.to_canonical_toon()));
            }

            let Some(document) = value.as_object() else {
                return Ok("null\n".to_owned());
            };
            current = document;
        }
    }

    Err(format!("unsupported query `{query}`"))
}
