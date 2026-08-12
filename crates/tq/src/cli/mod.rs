use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Cursor, Read};
use std::process::ExitCode;

use reddb_io_toon::{
    close_transform_stream, close_transform_stream_interleaved, decode_with_options,
    detect_toonl_truncation, detect_truncation_with_options, Array, DecodeOptions, ToonlReader, Value,
};

mod args;
mod output;
mod token_stats;
mod toonl_trim;
mod upgrade;
mod xml;

use crate::query::{Halt, Inputs, Variables};
use args::{
    parse_args, parse_check_args, parse_close_args, parse_trim_args, CheckOptions, CloseOptions,
    Format, Options, TrimOptions,
};
use output::format_values;
use toonl_trim::{trim_toonl_keep_last, write_in_place_atomically};
use upgrade::{parse_upgrade_args, run_upgrade};
use xml::parse_xml_value;

pub(crate) fn main() -> ExitCode {
    match run() {
        Ok((output, code)) => {
            print!("{output}");
            code
        }
        // A `halt` that reached here produced nothing to write, so only its
        // message and exit status are left to deliver.
        Err(error) => match Halt::decode(&error) {
            Some(halt) => report_halt(&halt),
            None => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

fn report_halt(halt: &Halt) -> ExitCode {
    eprint!("{}", halt.message);
    ExitCode::from(halt.code)
}

fn run() -> Result<(String, ExitCode), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| arg == "-V" || arg == "--version")
    {
        return Ok((
            format!("tq {}\n", env!("CARGO_PKG_VERSION")),
            ExitCode::SUCCESS,
        ));
    }
    if args.first().is_some_and(|arg| arg == "trim") {
        return run_trim(parse_trim_args(args.into_iter().skip(1))?)
            .map(|output| (output, ExitCode::SUCCESS));
    }
    if args.first().is_some_and(|arg| arg == "close") {
        return run_close(parse_close_args(args.into_iter().skip(1))?)
            .map(|output| (output, ExitCode::SUCCESS));
    }
    if args.first().is_some_and(|arg| arg == "check") {
        return run_check(parse_check_args(args.into_iter().skip(1))?);
    }
    if args.first().is_some_and(|arg| arg == "upgrade") {
        return run_upgrade(parse_upgrade_args(args.into_iter().skip(1))?);
    }

    let options = parse_args(args.into_iter())?;
    let variables = Variables::new(&options.variables);

    // TOONL comes first because its reader is also the stream `input`/`inputs`
    // read from, which `-n` leaves untouched rather than unavailable.
    if options.input_format == Format::Toonl && !options.raw_input {
        return run_toonl(&options, &variables);
    }
    if options.null_input {
        let values = crate::query::evaluate(&Value::Null, &options.query, &variables)?;
        return finish(values, &options);
    }
    if options.raw_input {
        return run_raw_input(&options, &variables);
    }

    let input = read_input(&options)?;
    let input_format = if options.input_format == Format::Toon
        && !options.input_format_explicit
        && looks_like_xml(&input)
    {
        Format::Xml
    } else {
        options.input_format
    };
    let values = match input_format {
        Format::Json => {
            let document = Value::from_json_str(&input).map_err(|error| error.to_string())?;
            crate::query::evaluate(&document, &options.query, &variables)?
        }
        Format::Yaml => {
            let document = parse_yaml_value(&input)?;
            crate::query::evaluate(&document, &options.query, &variables)?
        }
        Format::Toon => {
            let document = decode_with_options(
                &input,
                &DecodeOptions {
                    indent: options.indent_size,
                    strict: options.strict,
                    cyclic_discriminated_arrays: options.cyclic_discriminated_arrays,
                    ..DecodeOptions::default()
                },
            )
            .map_err(|error| error.to_string())?;
            crate::query::evaluate(&document, &options.query, &variables)?
        }
        Format::Xml => {
            let document = parse_xml_value(&input)?;
            crate::query::evaluate(&document, &options.query, &variables)?
        }
        Format::Toonl => unreachable!("TOONL input is handled before reading into a string"),
    };
    let code = output_exit_code(&values, options.exit_status);
    let output = format_values(&values, &options)?;
    if options.stats && input_format == Format::Json && options.output_format == Format::Toon {
        eprint!("{}", token_stats::format_statistics(&input, &output));
    }
    Ok((output, code))
}

fn output_exit_code(values: &[Value], enabled: bool) -> ExitCode {
    if !enabled {
        return ExitCode::SUCCESS;
    }
    match values.last() {
        None => ExitCode::from(4),
        Some(Value::Bool(false) | Value::Null) => ExitCode::FAILURE,
        Some(_) => ExitCode::SUCCESS,
    }
}

fn run_trim(options: TrimOptions) -> Result<String, String> {
    let input = match &options.input_path {
        Some(path) => fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?,
        None => read_stdin()?,
    };
    let plan = trim_toonl_keep_last(&input, options.keep_last)?;

    if options.in_place {
        let path = options
            .input_path
            .as_deref()
            .ok_or_else(|| "--in-place requires FILE".to_owned())?;
        if plan.changed {
            write_in_place_atomically(path, plan.output.as_bytes())?;
        }
        Ok(String::new())
    } else {
        Ok(plan.output)
    }
}

fn run_close(options: CloseOptions) -> Result<String, String> {
    let input = match &options.input_path {
        Some(path) => fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?,
        None => read_stdin()?,
    };
    let mut output = Vec::new();
    if options.interleaved {
        close_transform_stream_interleaved(Cursor::new(input.as_bytes()), &mut output)
    } else {
        close_transform_stream(Cursor::new(input.as_bytes()), &mut output)
    }
    .map_err(|error| error.to_string())?;
    String::from_utf8(output).map_err(|error| error.to_string())
}

fn run_check(options: CheckOptions) -> Result<(String, ExitCode), String> {
    let input = match &options.input_path {
        Some(path) => fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?,
        None => read_stdin()?,
    };
    let report = match options.input_format {
        Format::Toon => detect_truncation_with_options(&input, &DecodeOptions::default()),
        Format::Toonl => detect_toonl_truncation(&input),
        Format::Json | Format::Xml | Format::Yaml => {
            unreachable!("check rejects non-TOON input")
        }
    };
    let output =
        serde_json::to_string_pretty(&report.to_json_value()).map_err(|error| error.to_string())?;
    let code = if report.complete {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    };
    Ok((format!("{output}\n"), code))
}

fn run_toonl(options: &Options, variables: &Variables) -> Result<(String, ExitCode), String> {
    let reader = input_reader(options)?;
    let inputs =
        Inputs::new(ToonlReader::new(reader).map(|row| row.map_err(|error| error.to_string())));

    if options.slurp {
        let mut rows = Vec::new();
        while let Some(row) = inputs.next_input() {
            rows.push(row?);
        }
        let document = Value::Array(Array::List(rows));
        return evaluate_row(&document, options, variables, &inputs, Vec::new());
    }
    // `-n` runs the filter once against null and leaves every row for
    // `input`/`inputs` to draw, exactly as jq's `-n` does over a stream.
    if options.null_input {
        return evaluate_row(&Value::Null, options, variables, &inputs, Vec::new());
    }

    let mut values = Vec::new();
    while let Some(row) = inputs.next_input() {
        let row = row?;
        match crate::query::evaluate_reading(&row, &options.query, variables, Some(&inputs)) {
            Ok(produced) => values.extend(produced),
            Err(error) => return halted(error, values, options),
        }
    }

    finish(values, options)
}

fn evaluate_row(
    document: &Value,
    options: &Options,
    variables: &Variables,
    inputs: &Inputs,
    produced: Vec<Value>,
) -> Result<(String, ExitCode), String> {
    match crate::query::evaluate_reading(document, &options.query, variables, Some(inputs)) {
        Ok(values) => finish(values, options),
        Err(error) => halted(error, produced, options),
    }
}

/// A halted run still writes what it produced before the halt, then exits with
/// the status the filter asked for. Anything else is an ordinary error.
fn halted(
    error: String,
    values: Vec<Value>,
    options: &Options,
) -> Result<(String, ExitCode), String> {
    let Some(halt) = Halt::decode(&error) else {
        return Err(error);
    };
    eprint!("{}", halt.message);
    format_values(&values, options).map(|output| (output, ExitCode::from(halt.code)))
}

/// `--raw-input` replaces decoding entirely: each line is one string document,
/// or the whole input is a single string under `--slurp`.
fn run_raw_input(options: &Options, variables: &Variables) -> Result<(String, ExitCode), String> {
    let input = read_input(options)?;

    if options.slurp {
        let document = Value::String(input);
        let values = crate::query::evaluate(&document, &options.query, variables);
        return match values {
            Ok(values) => finish(values, options),
            Err(error) => halted(error, Vec::new(), options),
        };
    }

    let lines = raw_input_lines(&input)
        .into_iter()
        .map(|line| Ok(Value::String(line.to_owned())))
        .collect::<Vec<_>>();
    let inputs = Inputs::new(lines.into_iter());

    let mut values = Vec::new();
    while let Some(line) = inputs.next_input() {
        let document = line?;
        match crate::query::evaluate_reading(&document, &options.query, variables, Some(&inputs)) {
            Ok(produced) => values.extend(produced),
            Err(error) => return halted(error, values, options),
        }
    }

    finish(values, options)
}

/// jq's `--raw-input` line split: a trailing newline ends the last line instead
/// of starting an empty one, and empty input holds no lines at all.
fn raw_input_lines(input: &str) -> Vec<&str> {
    if input.is_empty() {
        return Vec::new();
    }
    input
        .strip_suffix('\n')
        .unwrap_or(input)
        .split('\n')
        .collect()
}

fn finish(values: Vec<Value>, options: &Options) -> Result<(String, ExitCode), String> {
    let code = output_exit_code(&values, options.exit_status);
    format_values(&values, options).map(|output| (output, code))
}

fn parse_yaml_value(input: &str) -> Result<Value, String> {
    let value = serde_norway::from_str(input).map_err(|error| error.to_string())?;
    Ok(Value::from_json_value(value))
}

fn looks_like_xml(input: &str) -> bool {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).trim_start();
    let Some(after_open) = input.strip_prefix('<') else {
        return false;
    };
    matches!(
        after_open.chars().next(),
        Some('!' | '?' | ':' | '_' | 'A'..='Z' | 'a'..='z')
    )
}

fn read_stdin() -> Result<String, String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("stdin: {error}"))?;
    Ok(input)
}

fn read_input(options: &Options) -> Result<String, String> {
    match &options.input_path {
        Some(path) => fs::read_to_string(path).map_err(|error| format!("{path}: {error}")),
        None => read_stdin(),
    }
}

fn input_reader(options: &Options) -> Result<Box<dyn BufRead>, String> {
    match &options.input_path {
        Some(path) => fs::File::open(path)
            .map(|file| Box::new(BufReader::new(file)) as Box<dyn BufRead>)
            .map_err(|error| format!("{path}: {error}")),
        None => Ok(Box::new(BufReader::new(io::stdin()))),
    }
}
