use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};

use reddb_io_toon::{
    close_transform_stream, close_transform_stream_interleaved, encode_toonl_values, Array,
    EncodeOptions, ToonlReader, Value,
};

const USAGE: &str =
    "usage: tq [-p toon|json|toonl] [-o toon|json|toonl] [-r] [-c] [-s|--slurp] [--nested-tabular-headers] [--keyed-map-collapse] <query> [file]";
const TRIM_USAGE: &str = "usage: tq trim --keep-last N [--in-place] [FILE]";
const CLOSE_USAGE: &str = "usage: tq close [--per-lane|--interleaved] [FILE]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Toon,
    Toonl,
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
    nested_tabular_headers: bool,
    keyed_map_collapse: bool,
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
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| arg == "-V" || arg == "--version")
    {
        return Ok(format!("tq {}\n", env!("CARGO_PKG_VERSION")));
    }
    if args.first().is_some_and(|arg| arg == "trim") {
        return run_trim(parse_trim_args(args.into_iter().skip(1))?);
    }
    if args.first().is_some_and(|arg| arg == "close") {
        return run_close(parse_close_args(args.into_iter().skip(1))?);
    }

    let options = parse_args(args.into_iter())?;

    if options.input_format == Format::Toonl {
        return run_toonl(&options);
    }

    let input = read_input(&options)?;
    let values = match options.input_format {
        Format::Json => {
            let document = Value::from_json_str(&input).map_err(|error| error.to_string())?;
            evaluate(&document, &options.query)?
        }
        Format::Toon => {
            let document = Value::parse_toon(&input).map_err(|error| error.to_string())?;
            evaluate(&document, &options.query)?
        }
        Format::Toonl => unreachable!("TOONL input is handled before reading into a string"),
    };
    format_values(&values, &options)
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
    let mut nested_tabular_headers = false;
    let mut keyed_map_collapse = false;
    let mut positional = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => {
                let format = args.next().ok_or_else(|| USAGE.to_owned())?;
                input_format = Some(parse_format(&format)?);
            }
            "-o" => {
                let format = args.next().ok_or_else(|| USAGE.to_owned())?;
                output_format = Some(parse_format(&format)?);
            }
            "-r" => raw_output = true,
            "-c" => compact = true,
            "-s" | "--slurp" => slurp = true,
            "--nested-tabular-headers" => nested_tabular_headers = true,
            "--keyed-map-collapse" => keyed_map_collapse = true,
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
        output_format: output_format.unwrap_or(input_format),
        raw_output,
        compact,
        slurp,
        nested_tabular_headers,
        keyed_map_collapse,
    })
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

fn parse_format(value: &str) -> Result<Format, String> {
    match value {
        "json" => Ok(Format::Json),
        "toon" => Ok(Format::Toon),
        "toonl" => Ok(Format::Toonl),
        _ => Err(format!("unsupported format `{value}`")),
    }
}

fn detect_input_format(path: Option<&str>) -> Format {
    if path.is_some_and(|path| path.ends_with(".toonl")) {
        Format::Toonl
    } else {
        Format::Toon
    }
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

fn trim_toonl_keep_last(input: &str, keep_last: usize) -> Result<TrimPlan, String> {
    validate_toonl(input)?;
    let scan = scan_toonl_trim_units(input)?;

    if scan.rows.len() <= keep_last {
        return Ok(TrimPlan {
            output: input.to_owned(),
            changed: false,
        });
    }

    let (headers, suffix_start) = if keep_last == 0 {
        (scan.live_headers_at_end.clone(), input.len())
    } else {
        let cut_index = scan.rows.len() - keep_last;
        let cut = &scan.rows[cut_index];
        (cut.live_headers.clone(), cut.start)
    };

    let mut output = String::new();
    for header in &headers {
        output.push_str(&line_with_lf(header));
    }
    if keep_last == 0 {
        if scan
            .last_anonymous_segment
            .and_then(|segment| scan.segments.get(segment))
            .and_then(|segment| segment.trailer)
            .is_some()
        {
            output.push_str("[=0]\n");
        }
    } else {
        append_trimmed_suffix(input, suffix_start, &scan, &mut output);
    }
    validate_toonl(&output)?;

    Ok(TrimPlan {
        changed: output != input,
        output,
    })
}

#[derive(Debug)]
struct TrimScan {
    segments: Vec<TrimSegment>,
    rows: Vec<TrimRow>,
    live_headers_at_end: Vec<String>,
    last_anonymous_segment: Option<usize>,
}

fn validate_toonl(input: &str) -> Result<(), String> {
    for row in ToonlReader::new(Cursor::new(input.as_bytes())) {
        row.map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn scan_toonl_trim_units(input: &str) -> Result<TrimScan, String> {
    let mut segments: Vec<TrimSegment> = Vec::new();
    let mut rows: Vec<TrimRow> = Vec::new();
    let mut current: Option<usize> = None;
    let mut last_segment: Option<usize> = None;
    let mut live_headers = LiveHeaders::default();
    let mut offset = 0;
    let mut line_number = 0;

    while offset < input.len() {
        let line_start = offset;
        let line_end = input[offset..]
            .find('\n')
            .map(|index| offset + index + 1)
            .unwrap_or(input.len());
        offset = line_end;
        line_number += 1;

        let raw_line = &input[line_start..line_end];
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if is_toonl_trailer(line) {
            let segment = current
                .take()
                .ok_or_else(|| format!("line {line_number}: trailer without header"))?;
            segments[segment].trailer = Some((line_start, line_end));
            continue;
        }
        if let Some(header) = parse_toonl_trim_header(line) {
            match header {
                TrimHeader::Continuation => {}
                TrimHeader::Anonymous => {
                    let segment = segments.len();
                    segments.push(TrimSegment {
                        header_start: line_start,
                        trailer: None,
                    });
                    current = Some(segment);
                    last_segment = Some(segment);
                    live_headers.set_anonymous(raw_line.to_owned());
                }
                TrimHeader::Tagged(tag) => {
                    live_headers.set_tagged(tag, raw_line.to_owned());
                }
            }
            continue;
        }

        let anonymous_segment = if is_toonl_tagged_row(line, &live_headers) {
            None
        } else {
            Some(current.ok_or_else(|| format!("line {line_number}: row before header"))?)
        };
        rows.push(TrimRow {
            start: line_start,
            live_headers: live_headers.lines(),
            anonymous_segment,
        });
    }

    Ok(TrimScan {
        segments,
        rows,
        live_headers_at_end: live_headers.lines(),
        last_anonymous_segment: last_segment,
    })
}

fn is_toonl_trailer(line: &str) -> bool {
    line.starts_with("[=") && line.ends_with(']')
}

fn parse_toonl_trim_header(line: &str) -> Option<TrimHeader> {
    let rest = line.strip_prefix('[')?;
    let close_bracket = rest.find(']')?;
    let bracket = &rest[..close_bracket];
    let continuation = bracket.starts_with('~');
    let delimiter = if continuation { &bracket[1..] } else { bracket };
    if !matches!(delimiter, "" | "|" | "\t") {
        return None;
    }
    let mut suffix = &rest[close_bracket + 1..];
    if continuation {
        return if suffix.starts_with('{') && suffix.ends_with("}:") {
            Some(TrimHeader::Continuation)
        } else {
            None
        };
    }
    if let Some(after_open) = suffix.strip_prefix('<') {
        let tag_end = after_open.find('>')?;
        let tag = &after_open[..tag_end];
        suffix = &after_open[tag_end + 1..];
        return if suffix.starts_with('{') && suffix.ends_with("}:") {
            Some(TrimHeader::Tagged(tag.to_owned()))
        } else {
            None
        };
    }
    if suffix.starts_with('{') && suffix.ends_with("}:") {
        Some(TrimHeader::Anonymous)
    } else {
        None
    }
}

fn is_toonl_tagged_row(line: &str, live_headers: &LiveHeaders) -> bool {
    let Some(colon) = line.find(':') else {
        return false;
    };
    if colon == 0 {
        return false;
    }
    let tag = &line[..colon];
    live_headers.has_tag(tag)
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn append_trimmed_suffix(input: &str, suffix_start: usize, scan: &TrimScan, output: &mut String) {
    let mut cursor = suffix_start;
    for (segment_index, segment) in scan.segments.iter().enumerate() {
        let Some((trailer_start, trailer_end)) = segment.trailer else {
            continue;
        };
        if trailer_start < suffix_start || segment.header_start >= suffix_start {
            continue;
        }
        output.push_str(&input[cursor..trailer_start]);
        let retained = scan
            .rows
            .iter()
            .filter(|row| row.start >= suffix_start && row.anonymous_segment == Some(segment_index))
            .count();
        output.push_str(&format!("[={retained}]\n"));
        cursor = trailer_end;
    }
    output.push_str(&input[cursor..]);
}

#[derive(Debug)]
enum TrimHeader {
    Anonymous,
    Continuation,
    Tagged(String),
}

#[derive(Debug, Default)]
struct LiveHeaders {
    order: Vec<LiveHeaderKey>,
    anonymous: Option<String>,
    tagged: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LiveHeaderKey {
    Anonymous,
    Tagged(String),
}

impl LiveHeaders {
    fn set_anonymous(&mut self, header: String) {
        if self.anonymous.is_none() {
            self.order.push(LiveHeaderKey::Anonymous);
        }
        self.anonymous = Some(header);
    }

    fn set_tagged(&mut self, tag: String, header: String) {
        if let Some((_, existing)) = self
            .tagged
            .iter_mut()
            .find(|(existing_tag, _)| existing_tag == &tag)
        {
            *existing = header;
            return;
        }
        self.order.push(LiveHeaderKey::Tagged(tag.clone()));
        self.tagged.push((tag, header));
    }

    fn has_tag(&self, tag: &str) -> bool {
        self.tagged
            .iter()
            .any(|(existing_tag, _)| existing_tag == tag)
    }

    fn lines(&self) -> Vec<String> {
        self.order
            .iter()
            .filter_map(|key| match key {
                LiveHeaderKey::Anonymous => self.anonymous.clone(),
                LiveHeaderKey::Tagged(tag) => self
                    .tagged
                    .iter()
                    .find(|(existing_tag, _)| existing_tag == tag)
                    .map(|(_, header)| header.clone()),
            })
            .collect()
    }
}

fn line_with_lf(line: &str) -> String {
    if line.ends_with('\n') {
        line.to_owned()
    } else {
        format!("{line}\n")
    }
}

fn write_in_place_atomically(path: &str, bytes: &[u8]) -> Result<(), String> {
    let path = Path::new(path);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "input path must name a file".to_owned())?;

    let mut last_error = None;
    for attempt in 0..100 {
        let tmp_path = parent.join(format!(
            ".{file_name}.tq-trim.{}.{}.tmp",
            process::id(),
            attempt
        ));
        match write_temp_then_rename(path, &tmp_path, bytes) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                last_error = Some(error);
            }
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                return Err(format!("{}: {error}", path.display()));
            }
        }
    }

    Err(format!(
        "{}: could not create temporary trim file: {}",
        path.display(),
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "too many collisions".to_owned())
    ))
}

fn write_temp_then_rename(path: &Path, tmp_path: &PathBuf, bytes: &[u8]) -> io::Result<()> {
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, path)
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
    Add,
    FromEntries,
    GroupBy(Box<Expr>),
    Has(Box<Expr>),
    Join(Box<Expr>),
    Keys,
    Length,
    Map(Box<Expr>),
    MaxBy(Box<Expr>),
    MinBy(Box<Expr>),
    Select(Box<Expr>),
    SortBy(Box<Expr>),
    Split(Box<Expr>),
    Test(Box<Expr>),
    ToEntries,
    Unique,
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
        Builtin::Add => evaluate_add(input).map(|value| vec![value]),
        Builtin::FromEntries => evaluate_from_entries(input).map(|value| vec![value]),
        Builtin::GroupBy(filter) => evaluate_group_by(input, filter).map(|value| vec![value]),
        Builtin::Has(key_filter) => {
            let keys = key_filter.eval(input)?;
            keys.into_iter()
                .map(|key| evaluate_has(input, &key))
                .collect()
        }
        Builtin::Join(separator_filter) => {
            evaluate_join(input, &single_string_arg(separator_filter, input, "join")?)
                .map(|value| vec![value])
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
        Builtin::MaxBy(filter) => evaluate_min_max_by(input, filter, true).map(|value| vec![value]),
        Builtin::MinBy(filter) => {
            evaluate_min_max_by(input, filter, false).map(|value| vec![value])
        }
        Builtin::SortBy(filter) => evaluate_sort_by(input, filter).map(|value| vec![value]),
        Builtin::Split(separator_filter) => {
            evaluate_split(input, &single_string_arg(separator_filter, input, "split")?)
                .map(|value| vec![value])
        }
        Builtin::Test(pattern_filter) => {
            evaluate_test(input, &single_string_arg(pattern_filter, input, "test")?)
                .map(|value| vec![value])
        }
        Builtin::ToEntries => evaluate_to_entries(input).map(|value| vec![value]),
        Builtin::Unique => evaluate_unique(input).map(|value| vec![value]),
    }
}

fn evaluate_add(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("add cannot be applied to this value".to_owned());
    };

    let mut values = array.values().into_iter();
    let Some(mut total) = values.next() else {
        return Ok(Value::Null);
    };
    for value in values {
        total = add_values(&total, &value)?;
    }
    Ok(total)
}

fn evaluate_sort_by(input: &Value, filter: &Expr) -> Result<Value, String> {
    let mut keyed = keyed_array_values(input, filter)?;
    keyed.sort_by(|left, right| compare_key_json(&left.0, &right.0));
    Ok(Value::Array(Array::List(
        keyed.into_iter().map(|(_, value)| value).collect(),
    )))
}

fn evaluate_group_by(input: &Value, filter: &Expr) -> Result<Value, String> {
    let mut keyed = keyed_array_values(input, filter)?;
    keyed.sort_by(|left, right| compare_key_json(&left.0, &right.0));

    let mut groups: Vec<Value> = Vec::new();
    let mut current_key: Option<serde_json::Value> = None;
    let mut current_values = Vec::new();
    for (key, value) in keyed {
        if current_key.as_ref().is_some_and(|current| *current != key) {
            groups.push(Value::Array(Array::List(std::mem::take(
                &mut current_values,
            ))));
        }
        current_key = Some(key);
        current_values.push(value);
    }
    if current_key.is_some() {
        groups.push(Value::Array(Array::List(current_values)));
    }

    Ok(Value::Array(Array::List(groups)))
}

fn evaluate_unique(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("unique cannot be applied to this value".to_owned());
    };

    let mut values = array.values();
    values.sort_by(|left, right| compare_key_json(&left.to_json_value(), &right.to_json_value()));
    values.dedup_by(|left, right| left.to_json_value() == right.to_json_value());
    Ok(Value::Array(Array::List(values)))
}

fn evaluate_min_max_by(input: &Value, filter: &Expr, max: bool) -> Result<Value, String> {
    let keyed = keyed_array_values(input, filter)?;
    let selected = if max {
        keyed
            .into_iter()
            .max_by(|left, right| compare_key_json(&left.0, &right.0))
    } else {
        keyed
            .into_iter()
            .min_by(|left, right| compare_key_json(&left.0, &right.0))
    };
    Ok(selected.map(|(_, value)| value).unwrap_or(Value::Null))
}

fn keyed_array_values(
    input: &Value,
    filter: &Expr,
) -> Result<Vec<(serde_json::Value, Value)>, String> {
    let Value::Array(array) = input else {
        return Err("cannot order non-array".to_owned());
    };

    array
        .values()
        .into_iter()
        .map(|value| {
            let key = sort_key(filter, &value)?;
            Ok((key, value))
        })
        .collect()
}

fn sort_key(filter: &Expr, input: &Value) -> Result<serde_json::Value, String> {
    let values = filter.eval(input)?;
    if values.len() == 1 {
        Ok(values
            .into_iter()
            .next()
            .expect("one sort key exists")
            .to_json_value())
    } else {
        Ok(serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| value.to_json_value())
                .collect(),
        ))
    }
}

fn compare_key_json(left: &serde_json::Value, right: &serde_json::Value) -> std::cmp::Ordering {
    compare_json_values(left, right).unwrap_or(std::cmp::Ordering::Equal)
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

fn evaluate_to_entries(input: &Value) -> Result<Value, String> {
    let entries = match input {
        Value::Array(array) => array
            .values()
            .into_iter()
            .enumerate()
            .map(|(index, value)| entry_value(Value::Number(index.to_string()), value))
            .collect(),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            map.into_iter()
                .map(|(key, value)| entry_value(Value::String(key), Value::from_json_value(value)))
                .collect()
        }
        _ => return Err("to_entries cannot be applied to this value".to_owned()),
    };
    Ok(Value::Array(Array::List(entries)))
}

fn entry_value(key: Value, value: Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("key".to_owned(), key.to_json_value());
    object.insert("value".to_owned(), value.to_json_value());
    Value::from_json_value(serde_json::Value::Object(object))
}

fn evaluate_from_entries(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("from_entries cannot be applied to this value".to_owned());
    };

    let mut object = serde_json::Map::new();
    for entry in array.values() {
        let Value::Object(document) = entry else {
            return Err("from_entries expects object entries".to_owned());
        };
        let key = document
            .get("key")
            .or_else(|| document.get("Key"))
            .or_else(|| document.get("name"))
            .or_else(|| document.get("Name"))
            .ok_or_else(|| "from_entries entry missing key".to_owned())?;
        let value = document
            .get("value")
            .or_else(|| document.get("Value"))
            .ok_or_else(|| "from_entries entry missing value".to_owned())?;
        object.insert(entry_key_string(key)?, value.to_json_value());
    }

    Ok(Value::from_json_value(serde_json::Value::Object(object)))
}

fn entry_key_string(value: &Value) -> Result<String, String> {
    match value {
        Value::Number(value) | Value::String(value) => Ok(value.clone()),
        _ => Err("from_entries keys must be strings or numbers".to_owned()),
    }
}

fn evaluate_split(input: &Value, separator: &str) -> Result<Value, String> {
    let Value::String(value) = input else {
        return Err("split cannot be applied to this value".to_owned());
    };

    let values = if separator.is_empty() {
        value
            .chars()
            .map(|character| Value::String(character.to_string()))
            .collect()
    } else {
        value
            .split(separator)
            .map(|part| Value::String(part.to_owned()))
            .collect()
    };
    Ok(Value::Array(Array::List(values)))
}

fn evaluate_join(input: &Value, separator: &str) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("join cannot be applied to this value".to_owned());
    };

    let parts = array
        .values()
        .into_iter()
        .map(|value| match value {
            Value::Bool(value) => Ok(value.to_string()),
            Value::Null => Ok(String::new()),
            Value::Number(value) | Value::String(value) => Ok(value),
            _ => Err("join cannot stringify this value".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::String(parts.join(separator)))
}

fn evaluate_test(input: &Value, pattern: &str) -> Result<Value, String> {
    let Value::String(value) = input else {
        return Err("test cannot be applied to this value".to_owned());
    };
    let regex = regex::Regex::new(pattern).map_err(|error| format!("invalid regex: {error}"))?;
    Ok(Value::Bool(regex.is_match(value)))
}

fn single_string_arg(filter: &Expr, input: &Value, builtin: &str) -> Result<String, String> {
    let values = filter.eval(input)?;
    match values.as_slice() {
        [Value::String(value)] => Ok(value.clone()),
        [_] => Err(format!("{builtin} argument must be a string")),
        _ => Err(format!("{builtin} argument must produce one value")),
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
            "add" => Ok(Expr::Builtin(Builtin::Add)),
            "false" => Ok(Expr::Literal(Value::Bool(false))),
            "from_entries" => Ok(Expr::Builtin(Builtin::FromEntries)),
            "group_by" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::GroupBy(Box::new(filter))))
            }
            "has" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Has(Box::new(filter))))
            }
            "join" => {
                self.expect(LexToken::LParen)?;
                let separator = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Join(Box::new(separator))))
            }
            "keys" => Ok(Expr::Builtin(Builtin::Keys)),
            "length" => Ok(Expr::Builtin(Builtin::Length)),
            "map" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Map(Box::new(filter))))
            }
            "max_by" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::MaxBy(Box::new(filter))))
            }
            "min_by" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::MinBy(Box::new(filter))))
            }
            "null" => Ok(Expr::Literal(Value::Null)),
            "select" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Select(Box::new(filter))))
            }
            "sort_by" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::SortBy(Box::new(filter))))
            }
            "split" => {
                self.expect(LexToken::LParen)?;
                let separator = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Split(Box::new(separator))))
            }
            "test" => {
                self.expect(LexToken::LParen)?;
                let pattern = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Test(Box::new(pattern))))
            }
            "to_entries" => Ok(Expr::Builtin(Builtin::ToEntries)),
            "true" => Ok(Expr::Literal(Value::Bool(true))),
            "unique" => Ok(Expr::Builtin(Builtin::Unique)),
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
    if options.output_format == Format::Toonl {
        return encode_toonl_values(values).map_err(|error| error.to_string());
    }

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
                output.push_str(&value.to_toon_with_options(EncodeOptions {
                    nested_tabular_headers: options.nested_tabular_headers,
                    keyed_map_collapse: options.keyed_map_collapse,
                }));
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }
            Format::Toonl => unreachable!("TOONL output is handled before the loop"),
        }
    }
    Ok(output)
}
