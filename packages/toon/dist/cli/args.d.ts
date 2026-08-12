/**
 * The `toon` CLI argument grammar, hand-rolled so the package stays
 * dependency-free: positional `[input]`, `-o/--output`, `-e/--encode`,
 * `-d/--decode`, `--delimiter`, `--indent`, `--strict`/`--no-strict`,
 * `--stats`, `--verbose`, plus the built-in `--help`/`--version`.
 */
export interface ParsedArgs {
    input?: string;
    output?: string;
    encode: boolean;
    decode: boolean;
    delimiter: string;
    indent: string;
    strict: boolean;
    stats: boolean;
    verbose: boolean;
    help: boolean;
    version: boolean;
}
export declare const HELP_TEXT = "TOON CLI \u2013 Convert between JSON and TOON\n\nUSAGE toon [options] [input]\n\nARGUMENTS\n\n  input                  Input file path (omit or use \"-\" to read from stdin)\n\nOPTIONS\n\n  -o, --output <file>    Output file path (prints to stdout if omitted)\n  -e, --encode           Encode JSON to TOON (auto-detected by default)\n  -d, --decode           Decode TOON to JSON (auto-detected by default)\n      --delimiter <c>    Delimiter for rows and inline arrays: comma (,), tab (\\t), or pipe (|)\n      --indent <number>  Indentation size (default: 2)\n      --strict           Strict decode validation (disable with --no-strict)\n      --stats            Show token statistics\n      --verbose          Print the cause chain and stack trace on failure\n  -h, --help             Show this help message\n  -v, --version          Show the version\n";
export declare function parseCliArgs(argv: readonly string[]): ParsedArgs;
