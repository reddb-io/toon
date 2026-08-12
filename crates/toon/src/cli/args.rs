//! The `toon` CLI argument grammar, hand-rolled so the crate keeps its single
//! `serde_json` dependency: positional `[input]`, `-o/--output`, `-e/--encode`,
//! `-d/--decode`, `--delimiter`, `--indent`, `--strict`/`--no-strict`,
//! `--stats`, `--verbose`, plus the built-in `--help`/`--version`.
//!
//! The TypeScript twin is `packages/toon/src/cli/args.ts`; the shared corpus
//! under `tests/golden/toon-cli/` is the contract between the two ports.

use super::errors::CliError;

/// One parsed invocation. Values stay as text so the run boundary reports the
/// spelling the user typed when it rejects them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedArgs {
    pub input: Option<String>,
    pub output: Option<String>,
    pub encode: bool,
    pub decode: bool,
    pub delimiter: String,
    pub indent: String,
    pub strict: bool,
    pub stats: bool,
    pub verbose: bool,
    pub help: bool,
    pub version: bool,
}

impl Default for ParsedArgs {
    fn default() -> Self {
        Self {
            input: None,
            output: None,
            encode: false,
            decode: false,
            delimiter: ",".to_owned(),
            indent: "2".to_owned(),
            strict: true,
            stats: false,
            verbose: false,
            help: false,
            version: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Boolean,
    Value,
}

struct OptionDef {
    name: &'static str,
    alias: Option<char>,
    kind: Kind,
}

const OPTIONS: &[OptionDef] = &[
    OptionDef {
        name: "output",
        alias: Some('o'),
        kind: Kind::Value,
    },
    OptionDef {
        name: "encode",
        alias: Some('e'),
        kind: Kind::Boolean,
    },
    OptionDef {
        name: "decode",
        alias: Some('d'),
        kind: Kind::Boolean,
    },
    OptionDef {
        name: "delimiter",
        alias: None,
        kind: Kind::Value,
    },
    OptionDef {
        name: "indent",
        alias: None,
        kind: Kind::Value,
    },
    OptionDef {
        name: "strict",
        alias: None,
        kind: Kind::Boolean,
    },
    OptionDef {
        name: "stats",
        alias: None,
        kind: Kind::Boolean,
    },
    OptionDef {
        name: "verbose",
        alias: None,
        kind: Kind::Boolean,
    },
    OptionDef {
        name: "help",
        alias: Some('h'),
        kind: Kind::Boolean,
    },
    OptionDef {
        name: "version",
        alias: Some('v'),
        kind: Kind::Boolean,
    },
];

pub const HELP_TEXT: &str = concat!(
    "TOON CLI – Convert between JSON and TOON\n",
    "\n",
    "USAGE toon [options] [input]\n",
    "\n",
    "ARGUMENTS\n",
    "\n",
    "  input                  Input file path (omit or use \"-\" to read from stdin)\n",
    "\n",
    "OPTIONS\n",
    "\n",
    "  -o, --output <file>    Output file path (prints to stdout if omitted)\n",
    "  -e, --encode           Encode JSON to TOON (auto-detected by default)\n",
    "  -d, --decode           Decode TOON to JSON (auto-detected by default)\n",
    "      --delimiter <c>    Delimiter for rows and inline arrays: comma (,), tab (\\t), or pipe (|)\n",
    "      --indent <number>  Indentation size (default: 2)\n",
    "      --strict           Strict decode validation (disable with --no-strict)\n",
    "      --stats            Show token statistics\n",
    "      --verbose          Print the cause chain and stack trace on failure\n",
    "  -h, --help             Show this help message\n",
    "  -v, --version          Show the version\n",
);

pub fn parse_cli_args(argv: &[String]) -> Result<ParsedArgs, CliError> {
    let mut parsed = ParsedArgs::default();
    let mut positionals: Vec<&str> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    let mut only_positionals = false;
    let mut index = 0;

    while index < argv.len() {
        let token = argv[index].as_str();

        if only_positionals || token == "-" || !token.starts_with('-') {
            positionals.push(token);
        } else if token == "--" {
            only_positionals = true;
        } else if token.starts_with("--") {
            index = read_long_option(token, argv, index, &mut parsed, &mut unknown);
        } else {
            index = read_short_cluster(token, argv, index, &mut parsed, &mut unknown);
        }

        index += 1;
    }

    if let Some(first) = positionals.first() {
        parsed.input = Some((*first).to_owned());
    }
    for surplus in positionals.iter().skip(1) {
        unknown.push(quote(surplus));
    }

    if unknown.is_empty() {
        return Ok(parsed);
    }

    let mut seen: Vec<String> = Vec::new();
    for name in unknown {
        if !seen.contains(&name) {
            seen.push(name);
        }
    }
    Err(CliError::new(format!(
        "Unknown argument(s): {} – see --help",
        seen.join(", ")
    )))
}

/// Returns the index of the last argv token this option consumed.
fn read_long_option(
    token: &str,
    argv: &[String],
    index: usize,
    parsed: &mut ParsedArgs,
    unknown: &mut Vec<String>,
) -> usize {
    let (name, inline_value) = match token.find('=') {
        None => (&token[2..], None),
        Some(equals) => (&token[2..equals], Some(&token[equals + 1..])),
    };

    let negated = find_option(name).is_none() && name.starts_with("no-");
    let Some(option) = find_option(if negated { &name[3..] } else { name }) else {
        unknown.push(format!("--{name}"));
        return index;
    };

    if option.kind == Kind::Boolean {
        let enabled = !negated && !matches!(inline_value, Some("false") | Some("0"));
        assign_boolean(parsed, option.name, enabled);
        return index;
    }

    if let Some(value) = inline_value {
        assign_value(parsed, option.name, value);
        return index;
    }

    match argv.get(index + 1) {
        Some(value) => {
            assign_value(parsed, option.name, value);
            index + 1
        }
        None => {
            assign_value(parsed, option.name, "");
            index
        }
    }
}

/// Reads `-e`, `-o value`, `-ovalue`, and clusters such as `-ed`.
fn read_short_cluster(
    token: &str,
    argv: &[String],
    index: usize,
    parsed: &mut ParsedArgs,
    unknown: &mut Vec<String>,
) -> usize {
    let cluster: Vec<char> = token.chars().skip(1).collect();

    for (position, letter) in cluster.iter().enumerate() {
        let Some(option) = OPTIONS.iter().find(|option| option.alias == Some(*letter)) else {
            unknown.push(format!("-{letter}"));
            continue;
        };
        if option.kind == Kind::Boolean {
            assign_boolean(parsed, option.name, true);
            continue;
        }

        let rest: String = cluster[position + 1..].iter().collect();
        if !rest.is_empty() {
            assign_value(parsed, option.name, rest.strip_prefix('=').unwrap_or(&rest));
            return index;
        }
        return match argv.get(index + 1) {
            Some(value) => {
                assign_value(parsed, option.name, value);
                index + 1
            }
            None => {
                assign_value(parsed, option.name, "");
                index
            }
        };
    }

    index
}

fn find_option(name: &str) -> Option<&'static OptionDef> {
    OPTIONS.iter().find(|option| option.name == name)
}

fn assign_boolean(parsed: &mut ParsedArgs, name: &str, value: bool) {
    match name {
        "encode" => parsed.encode = value,
        "decode" => parsed.decode = value,
        "strict" => parsed.strict = value,
        "stats" => parsed.stats = value,
        "verbose" => parsed.verbose = value,
        "help" => parsed.help = value,
        "version" => parsed.version = value,
        _ => {}
    }
}

fn assign_value(parsed: &mut ParsedArgs, name: &str, value: &str) {
    match name {
        "output" => parsed.output = Some(value.to_owned()),
        "delimiter" => parsed.delimiter = value.to_owned(),
        "indent" => parsed.indent = value.to_owned(),
        _ => {}
    }
}

/// `JSON.stringify(text)`, which is how upstream quotes a surplus positional.
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}
