use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};

use reddb_io_toon::{
    close_transform_stream, close_transform_stream_interleaved, detect_toonl_truncation,
    detect_truncation_with_options, encode_toonl_values, Array, EncodeOptions, ParseOptions,
    ToonlReader, Value,
};

const USAGE: &str =
    "usage: tq [-p toon|json|toonl|yaml|yml] [-o toon|json|toonl] [-r] [-c] [-s|--slurp] [--delimiter comma|tab|pipe] [--nested-tabular-headers] [--keyed-map-collapse] [--primitive-array-columns] [--object-array-columns] [--cyclic-discriminated-arrays] <query> [file]";
const TRIM_USAGE: &str = "usage: tq trim --keep-last N [--in-place] [FILE]";
const CLOSE_USAGE: &str = "usage: tq close [--per-lane|--interleaved] [FILE]";
const CHECK_USAGE: &str = "usage: tq check [-p toon|toonl] [FILE]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Toon,
    Toonl,
    Yaml,
}

#[derive(Debug)]
struct Options {
    query: String,
    input_path: Option<String>,
    input_format: Format,
    output_format: Format,
    raw_output: bool,
    compact: bool,
    slurp: bool,
    delimiter: char,
    nested_tabular_headers: bool,
    keyed_map_collapse: bool,
    primitive_array_columns: bool,
    object_array_columns: bool,
    cyclic_discriminated_arrays: bool,
}

#[derive(Debug)]
struct TrimOptions {
    keep_last: usize,
    in_place: bool,
    input_path: Option<String>,
}

#[derive(Debug)]
struct CloseOptions {
    interleaved: bool,
    input_path: Option<String>,
}

#[derive(Debug)]
struct CheckOptions {
    input_path: Option<String>,
    input_format: Format,
}

#[derive(Debug)]
struct TrimPlan {
    output: String,
    changed: bool,
}

#[derive(Debug)]
struct TrimSegment {
    header_start: usize,
    trailer: Option<(usize, usize)>,
}

#[derive(Debug)]
struct TrimRow {
    start: usize,
    live_headers: Vec<String>,
    anonymous_segment: Option<usize>,
}

fn main() -> ExitCode {
    match run() {
        Ok((output, code)) => {
            print!("{output}");
            code
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
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

    let options = parse_args(args.into_iter())?;

    if options.input_format == Format::Toonl {
        return run_toonl(&options).map(|output| (output, ExitCode::SUCCESS));
    }

    let input = read_input(&options)?;
    let values = match options.input_format {
        Format::Json => {
            let document = Value::from_json_str(&input).map_err(|error| error.to_string())?;
            evaluate(&document, &options.query)?
        }
        Format::Yaml => {
            let document = parse_yaml_value(&input)?;
            evaluate(&document, &options.query)?
        }
        Format::Toon => {
            let document = Value::parse_toon(&input).map_err(|error| error.to_string())?;
            evaluate(&document, &options.query)?
        }
        Format::Toonl => unreachable!("TOONL input is handled before reading into a string"),
    };
    format_values(&values, &options).map(|output| (output, ExitCode::SUCCESS))
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
        Format::Toon => detect_truncation_with_options(&input, ParseOptions::default()),
        Format::Toonl => detect_toonl_truncation(&input),
        Format::Json | Format::Yaml => unreachable!("check rejects non-TOON input"),
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

fn run_toonl(options: &Options) -> Result<String, String> {
    let reader = input_reader(options)?;
    let mut rows = Vec::new();
    let mut values = Vec::new();

    for row in ToonlReader::new(reader) {
        let row = row.map_err(|error| error.to_string())?;
        if options.slurp {
            rows.push(row);
        } else {
            values.extend(evaluate(&row, &options.query)?);
        }
    }

    if options.slurp {
        values = evaluate(&Value::Array(Array::List(rows)), &options.query)?;
    }

    format_values(&values, options)
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut input_format = None;
    let mut output_format = None;
    let mut raw_output = false;
    let mut compact = false;
    let mut slurp = false;
    let mut delimiter = ',';
    let mut nested_tabular_headers = false;
    let mut keyed_map_collapse = false;
    let mut primitive_array_columns = false;
    let mut object_array_columns = false;
    let mut cyclic_discriminated_arrays = false;
    let mut positional = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => {
                let format = args.next().ok_or_else(|| USAGE.to_owned())?;
                input_format = Some(parse_input_format(&format)?);
            }
            "-o" => {
                let format = args.next().ok_or_else(|| USAGE.to_owned())?;
                output_format = Some(parse_output_format(&format)?);
            }
            "-r" => raw_output = true,
            "-c" => compact = true,
            "-s" | "--slurp" => slurp = true,
            "--delimiter" => {
                let value = args.next().ok_or_else(|| USAGE.to_owned())?;
                delimiter = parse_delimiter(&value)?;
            }
            "--nested-tabular-headers" => nested_tabular_headers = true,
            "--keyed-map-collapse" => keyed_map_collapse = true,
            "--primitive-array-columns" => primitive_array_columns = true,
            "--object-array-columns" => object_array_columns = true,
            "--cyclic-discriminated-arrays" => cyclic_discriminated_arrays = true,
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

    let query = positional.remove(0);
    let input_path = positional.pop();
    let input_format = input_format.unwrap_or_else(|| detect_input_format(input_path.as_deref()));

    Ok(Options {
        query,
        input_path,
        input_format,
        output_format: output_format.unwrap_or_else(|| default_output_format(input_format)),
        raw_output,
        compact,
        slurp,
        delimiter,
        nested_tabular_headers,
        keyed_map_collapse,
        primitive_array_columns,
        object_array_columns,
        cyclic_discriminated_arrays,
    })
}

fn parse_delimiter(value: &str) -> Result<char, String> {
    match value {
        "comma" | "," => Ok(','),
        "tab" | "\\t" => Ok('\t'),
        "pipe" | "|" => Ok('|'),
        _ => Err("unsupported delimiter; expected comma, tab, or pipe".to_owned()),
    }
}

fn parse_trim_args(args: impl Iterator<Item = String>) -> Result<TrimOptions, String> {
    let mut keep_last = None;
    let mut in_place = false;
    let mut positional = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--keep-last" => {
                let value = args.next().ok_or_else(|| TRIM_USAGE.to_owned())?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| "`--keep-last` expects a non-negative integer".to_owned())?;
                keep_last = Some(parsed);
            }
            "--in-place" => in_place = true,
            "--" => {
                positional.extend(args);
                break;
            }
            value if value.starts_with('-') => return Err(TRIM_USAGE.to_owned()),
            value => positional.push(value.to_owned()),
        }
    }

    if positional.len() > 1 {
        return Err(TRIM_USAGE.to_owned());
    }
    if in_place && positional.is_empty() {
        return Err("--in-place requires FILE".to_owned());
    }

    Ok(TrimOptions {
        keep_last: keep_last.ok_or_else(|| TRIM_USAGE.to_owned())?,
        in_place,
        input_path: positional.pop(),
    })
}

fn parse_close_args(args: impl Iterator<Item = String>) -> Result<CloseOptions, String> {
    let mut interleaved = false;
    let mut positional = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--per-lane" => interleaved = false,
            "--interleaved" => interleaved = true,
            "--" => {
                positional.extend(args);
                break;
            }
            value if value.starts_with('-') => return Err(CLOSE_USAGE.to_owned()),
            value => positional.push(value.to_owned()),
        }
    }

    if positional.len() > 1 {
        return Err(CLOSE_USAGE.to_owned());
    }

    Ok(CloseOptions {
        interleaved,
        input_path: positional.pop(),
    })
}

fn parse_check_args(args: impl Iterator<Item = String>) -> Result<CheckOptions, String> {
    let mut input_format = None;
    let mut positional = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => {
                let format = args.next().ok_or_else(|| CHECK_USAGE.to_owned())?;
                let format = parse_input_format(&format)?;
                if matches!(format, Format::Json | Format::Yaml) {
                    return Err(CHECK_USAGE.to_owned());
                }
                input_format = Some(format);
            }
            "--" => {
                positional.extend(args);
                break;
            }
            value if value.starts_with('-') => return Err(CHECK_USAGE.to_owned()),
            value => positional.push(value.to_owned()),
        }
    }

    if positional.len() > 1 {
        return Err(CHECK_USAGE.to_owned());
    }

    let input_path = positional.pop();
    let input_format = input_format.unwrap_or_else(|| detect_input_format(input_path.as_deref()));
    if matches!(input_format, Format::Json | Format::Yaml) {
        return Err(CHECK_USAGE.to_owned());
    }
    Ok(CheckOptions {
        input_path,
        input_format,
    })
}

fn parse_input_format(value: &str) -> Result<Format, String> {
    match value {
        "yaml" | "yml" => Ok(Format::Yaml),
        _ => parse_output_format(value),
    }
}

fn parse_output_format(value: &str) -> Result<Format, String> {
    match value {
        "json" => Ok(Format::Json),
        "toon" => Ok(Format::Toon),
        "toonl" => Ok(Format::Toonl),
        _ => Err(format!("unsupported format `{value}`")),
    }
}

fn default_output_format(input_format: Format) -> Format {
    match input_format {
        Format::Yaml => Format::Toon,
        format => format,
    }
}

fn detect_input_format(path: Option<&str>) -> Format {
    if path.is_some_and(|path| path.ends_with(".toonl")) {
        Format::Toonl
    } else if path.is_some_and(|path| path.ends_with(".yaml") || path.ends_with(".yml")) {
        Format::Yaml
    } else {
        Format::Toon
    }
}

fn parse_yaml_value(input: &str) -> Result<Value, String> {
    let value = serde_norway::from_str(input).map_err(|error| error.to_string())?;
    Ok(Value::from_json_value(value))
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

