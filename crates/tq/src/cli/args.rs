use std::path::Path;

const USAGE: &str = concat!(
    "usage: tq [-p toon|json|toonl|yaml|yml|xml] [-o toon|json|toonl|xml] [-r] [-c] [-j] [-S] [-e] [-s|--slurp] [--stats] [--delimiter comma|tab|pipe] [--indent N] [--strict|--no-strict] [--nested-tabular-headers] [--keyed-map-collapse] [--primitive-array-columns] [--object-array-columns] [--cyclic-discriminated-arrays] <query> [file]\n",
    "subcommands: trim, close, check, upgrade"
);
const TRIM_USAGE: &str = "usage: tq trim --keep-last N [--in-place] [FILE]";
const CLOSE_USAGE: &str = "usage: tq close [--per-lane|--interleaved] [FILE]";
const CHECK_USAGE: &str = "usage: tq check [-p toon|toonl] [FILE]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Json,
    Toon,
    Toonl,
    Xml,
    Yaml,
}

#[derive(Debug)]
pub(super) struct Options {
    pub(super) query: String,
    pub(super) input_path: Option<String>,
    pub(super) input_format: Format,
    pub(super) input_format_explicit: bool,
    pub(super) output_format: Format,
    pub(super) raw_output: bool,
    pub(super) join_output: bool,
    pub(super) sort_keys: bool,
    pub(super) exit_status: bool,
    pub(super) compact: bool,
    pub(super) slurp: bool,
    pub(super) stats: bool,
    pub(super) delimiter: char,
    pub(super) indent_size: usize,
    pub(super) strict: bool,
    pub(super) primitive_array_columns: bool,
    pub(super) object_array_columns: bool,
    pub(super) cyclic_discriminated_arrays: bool,
}

#[derive(Debug)]
pub(super) struct TrimOptions {
    pub(super) keep_last: usize,
    pub(super) in_place: bool,
    pub(super) input_path: Option<String>,
}

#[derive(Debug)]
pub(super) struct CloseOptions {
    pub(super) interleaved: bool,
    pub(super) input_path: Option<String>,
}

#[derive(Debug)]
pub(super) struct CheckOptions {
    pub(super) input_path: Option<String>,
    pub(super) input_format: Format,
}

pub(super) fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut input_format = None;
    let mut output_format = None;
    let mut raw_output = false;
    let mut join_output = false;
    let mut sort_keys = false;
    let mut exit_status = false;
    let mut compact = false;
    let mut slurp = false;
    let mut stats = false;
    let mut delimiter = ',';
    let mut indent_size = 2;
    let mut strict = true;
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
            "-j" => join_output = true,
            "-S" => sort_keys = true,
            "-e" => exit_status = true,
            "-c" => compact = true,
            "-s" | "--slurp" => slurp = true,
            "--stats" => stats = true,
            "--delimiter" => {
                let value = args.next().ok_or_else(|| USAGE.to_owned())?;
                delimiter = parse_delimiter(&value)?;
            }
            "--indent" => {
                let value = args.next().ok_or_else(|| USAGE.to_owned())?;
                indent_size = parse_indent(&value)?;
            }
            "--strict" => strict = true,
            "--no-strict" => strict = false,
            // Canonical v4.1 output always uses nested and keyed tabular forms;
            // retain the old switches as deprecated no-ops for script compatibility.
            "--nested-tabular-headers" | "--keyed-map-collapse" => {}
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
    let input_format_explicit = input_format.is_some();
    let input_format = input_format.unwrap_or_else(|| detect_input_format(input_path.as_deref()));

    Ok(Options {
        query,
        input_path,
        input_format,
        input_format_explicit,
        output_format: output_format.unwrap_or_else(|| default_output_format(input_format)),
        raw_output,
        join_output,
        sort_keys,
        exit_status,
        compact,
        slurp,
        stats,
        delimiter,
        indent_size,
        strict,
        primitive_array_columns,
        object_array_columns,
        cyclic_discriminated_arrays,
    })
}

fn parse_indent(value: &str) -> Result<usize, String> {
    const ERROR: &str = "`--indent` expects a non-negative number";

    if value.is_empty() {
        return Ok(2);
    }

    let value = value.trim_start();
    let (negative, unsigned) = if let Some(rest) = value.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = value.strip_prefix('+') {
        (false, rest)
    } else {
        (false, value)
    };
    let digit_count = unsigned.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0 {
        return Err(ERROR.to_owned());
    }
    let indent = unsigned[..digit_count]
        .parse::<usize>()
        .map_err(|_| ERROR.to_owned())?;
    if negative && indent != 0 {
        return Err(ERROR.to_owned());
    }
    Ok(indent)
}

fn parse_delimiter(value: &str) -> Result<char, String> {
    match value {
        "comma" | "," => Ok(','),
        "tab" | "\\t" | "\t" => Ok('\t'),
        "pipe" | "|" => Ok('|'),
        _ => Err("unsupported delimiter; expected comma, tab, or pipe".to_owned()),
    }
}

pub(super) fn parse_trim_args(args: impl Iterator<Item = String>) -> Result<TrimOptions, String> {
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

pub(super) fn parse_close_args(args: impl Iterator<Item = String>) -> Result<CloseOptions, String> {
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

pub(super) fn parse_check_args(args: impl Iterator<Item = String>) -> Result<CheckOptions, String> {
    let mut input_format = None;
    let mut positional = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => {
                let format = args.next().ok_or_else(|| CHECK_USAGE.to_owned())?;
                let format = parse_input_format(&format)?;
                if matches!(format, Format::Json | Format::Xml | Format::Yaml) {
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
    if matches!(input_format, Format::Json | Format::Xml | Format::Yaml) {
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
        "xml" => Ok(Format::Xml),
        _ => Err(format!("unsupported format `{value}`")),
    }
}

fn default_output_format(input_format: Format) -> Format {
    match input_format {
        Format::Xml | Format::Yaml => Format::Toon,
        format => format,
    }
}

fn detect_input_format(path: Option<&str>) -> Format {
    match path
        .and_then(|path| Path::new(path).extension())
        .and_then(|value| value.to_str())
    {
        Some("json") => Format::Json,
        Some("toonl") => Format::Toonl,
        Some("xml") => Format::Xml,
        Some("yaml" | "yml") => Format::Yaml,
        _ => Format::Toon,
    }
}
