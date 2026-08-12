use std::path::Path;

const USAGE: &str = concat!(
    "usage: tq [-p toon|json|toonl|yaml|yml|xml] [-o toon|json|toonl|xml] [-r] [-c] [-j] [-S] [-e] [-n|--null-input] [-s|--slurp] [-R|--raw-input] [--arg name value] [--argjson name json] [--stats] [--delimiter comma|tab|pipe] [--indent N] [--strict|--no-strict] [--primitive-array-columns] [--object-array-columns] [--cyclic-discriminated-arrays] <query> [file]\n",
    "subcommands: trim, close, check, jq-check, upgrade"
);
const ARG_ERROR: &str = "`--arg` expects a variable name and a value";
const ARGJSON_ERROR: &str = "`--argjson` expects a variable name and JSON text";
const TRIM_USAGE: &str = "usage: tq trim --keep-last N [--in-place] [FILE]";
const CLOSE_USAGE: &str = "usage: tq close [--per-lane|--interleaved] [FILE]";
const CHECK_USAGE: &str = "usage: tq check [-p toon|toonl] [FILE]";
const JQ_CHECK_USAGE: &str = "usage: tq jq-check [jq option]... [--] <filter>";

/// One jq 1.7.1 command-line option tq honors with jq-compatible behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JqOption {
    Compact,
    RawOutput,
    JoinOutput,
    SortKeys,
    ExitStatus,
    NullInput,
    RawInput,
    Slurp,
    Arg,
    ArgJson,
}

impl JqOption {
    /// How many operands the option takes after its own name.
    const fn operands(self) -> usize {
        match self {
            Self::Arg | Self::ArgJson => 2,
            _ => 0,
        }
    }
}

/// The jq options tq accepts, with every spelling jq gives them. Argument
/// parsing and `tq jq-check` both dispatch from this one table, so the
/// compatibility decision cannot claim an option the parser rejects, and a
/// newly accepted option is classified the moment it lands here.
const JQ_OPTIONS: &[(&str, JqOption)] = &[
    ("-c", JqOption::Compact),
    ("-r", JqOption::RawOutput),
    ("-j", JqOption::JoinOutput),
    ("-S", JqOption::SortKeys),
    ("-e", JqOption::ExitStatus),
    ("-n", JqOption::NullInput),
    ("--null-input", JqOption::NullInput),
    ("-R", JqOption::RawInput),
    ("--raw-input", JqOption::RawInput),
    ("-s", JqOption::Slurp),
    ("--slurp", JqOption::Slurp),
    ("--arg", JqOption::Arg),
    ("--argjson", JqOption::ArgJson),
];

fn jq_option(argument: &str) -> Option<JqOption> {
    JQ_OPTIONS
        .iter()
        .find_map(|(name, option)| (*name == argument).then_some(*option))
}

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
    pub(super) null_input: bool,
    pub(super) raw_input: bool,
    pub(super) slurp: bool,
    /// Named `$variables` from `--arg` and `--argjson`, in flag order.
    pub(super) variables: Vec<(String, serde_json::Value)>,
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
    let mut null_input = false;
    let mut raw_input = false;
    let mut slurp = false;
    let mut variables = Vec::new();
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
        if let Some(option) = jq_option(&arg) {
            match option {
                JqOption::Compact => compact = true,
                JqOption::RawOutput => raw_output = true,
                JqOption::JoinOutput => join_output = true,
                JqOption::SortKeys => sort_keys = true,
                JqOption::ExitStatus => exit_status = true,
                JqOption::NullInput => null_input = true,
                JqOption::RawInput => raw_input = true,
                JqOption::Slurp => slurp = true,
                JqOption::Arg => {
                    let (name, value) = parse_named_pair(&mut args, ARG_ERROR)?;
                    variables.push((name, serde_json::Value::String(value)));
                }
                JqOption::ArgJson => {
                    let (name, text) = parse_named_pair(&mut args, ARGJSON_ERROR)?;
                    let value = serde_json::from_str(&text).map_err(|error| {
                        format!("`--argjson` value for `${name}` is not valid JSON: {error}")
                    })?;
                    variables.push((name, value));
                }
            }
            continue;
        }
        match arg.as_str() {
            "-p" => {
                let format = args.next().ok_or_else(|| USAGE.to_owned())?;
                input_format = Some(parse_input_format(&format)?);
            }
            "-o" => {
                let format = args.next().ok_or_else(|| USAGE.to_owned())?;
                output_format = Some(parse_output_format(&format)?);
            }
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
        null_input,
        raw_input,
        slurp,
        variables,
        stats,
        delimiter,
        indent_size,
        strict,
        primitive_array_columns,
        object_array_columns,
        cyclic_discriminated_arrays,
    })
}

/// Takes the two operands of `--arg`/`--argjson`. Like jq, both are consumed
/// verbatim, so a value that looks like a flag still binds to the variable.
fn parse_named_pair(
    args: &mut impl Iterator<Item = String>,
    error: &'static str,
) -> Result<(String, String), String> {
    let name = args.next().ok_or_else(|| error.to_owned())?;
    let value = args.next().ok_or_else(|| error.to_owned())?;
    Ok((name, value))
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

#[derive(Debug)]
pub(super) struct JqCheckOptions {
    pub(super) filter: String,
    /// Every option the invocation carried, in the order given.
    pub(super) options: Vec<String>,
    /// One detail per option tq does not honor with jq-compatible behavior.
    pub(super) rejected: Vec<String>,
}

/// `tq jq-check [jq option]... [--] <filter>`. The filter is the last argument,
/// or the single argument after `--`.
///
/// Only jq's own options and the `-p json`/`-o json` transport selection can
/// keep a decision positive. Anything else is reported rather than refused, so
/// a command proxy always reads a decision instead of a usage error.
pub(super) fn parse_jq_check_args(
    args: impl Iterator<Item = String>,
) -> Result<JqCheckOptions, String> {
    let args = args.collect::<Vec<_>>();
    let (leading, filter) = match args.iter().position(|arg| arg == "--") {
        Some(index) => {
            let tail = &args[index + 1..];
            if tail.len() != 1 {
                return Err(JQ_CHECK_USAGE.to_owned());
            }
            (&args[..index], tail[0].clone())
        }
        None => {
            let (last, leading) = args.split_last().ok_or_else(|| JQ_CHECK_USAGE.to_owned())?;
            (leading, last.clone())
        }
    };

    let mut options = Vec::new();
    let mut rejected = Vec::new();
    let mut index = 0;
    while index < leading.len() {
        let argument = &leading[index];
        let operand = leading.get(index + 1);

        let taken = if let Some(option) = jq_option(argument) {
            let operands = option.operands();
            if index + operands >= leading.len() {
                return Err(JQ_CHECK_USAGE.to_owned());
            }
            1 + operands
        } else if argument == "-p" || argument == "-o" {
            let value = operand.ok_or_else(|| JQ_CHECK_USAGE.to_owned())?;
            if value != "json" {
                rejected.push(format!(
                    "`{argument} {value}` selects a non-JSON transport; jq-compatible \
                     behavior needs `-p json -o json`"
                ));
            }
            2
        } else if argument.starts_with('-') && argument != "-" {
            rejected.push(format!("`{argument}` is not a jq 1.7.1 option tq honors"));
            1
        } else if rejected.is_empty() {
            // Nothing consumes it, so the invocation is not shaped as expected.
            return Err(JQ_CHECK_USAGE.to_owned());
        } else {
            // An operand of the option just reported; the decision is settled.
            1
        };

        options.extend(leading[index..index + taken].iter().cloned());
        index += taken;
    }

    Ok(JqCheckOptions {
        filter,
        options,
        rejected,
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
