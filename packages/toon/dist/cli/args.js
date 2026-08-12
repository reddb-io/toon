/**
 * The `toon` CLI argument grammar, hand-rolled so the package stays
 * dependency-free: positional `[input]`, `-o/--output`, `-e/--encode`,
 * `-d/--decode`, `--delimiter`, `--indent`, `--strict`/`--no-strict`,
 * `--stats`, `--verbose`, plus the built-in `--help`/`--version`.
 */
import { CliError } from './errors.js';
const OPTIONS = [
    { name: 'output', alias: 'o', kind: 'value' },
    { name: 'encode', alias: 'e', kind: 'boolean' },
    { name: 'decode', alias: 'd', kind: 'boolean' },
    { name: 'delimiter', kind: 'value' },
    { name: 'indent', kind: 'value' },
    { name: 'strict', kind: 'boolean' },
    { name: 'stats', kind: 'boolean' },
    { name: 'verbose', kind: 'boolean' },
    { name: 'help', alias: 'h', kind: 'boolean' },
    { name: 'version', alias: 'v', kind: 'boolean' },
];
const BY_NAME = new Map(OPTIONS.map((option) => [option.name, option]));
const BY_ALIAS = new Map(OPTIONS.filter((option) => option.alias).map((option) => [option.alias, option]));
export const HELP_TEXT = `TOON CLI – Convert between JSON and TOON

USAGE toon [options] [input]

ARGUMENTS

  input                  Input file path (omit or use "-" to read from stdin)

OPTIONS

  -o, --output <file>    Output file path (prints to stdout if omitted)
  -e, --encode           Encode JSON to TOON (auto-detected by default)
  -d, --decode           Decode TOON to JSON (auto-detected by default)
      --delimiter <c>    Delimiter for rows and inline arrays: comma (,), tab (\\t), or pipe (|)
      --indent <number>  Indentation size (default: 2)
      --strict           Strict decode validation (disable with --no-strict)
      --stats            Show token statistics
      --verbose          Print the cause chain and stack trace on failure
  -h, --help             Show this help message
  -v, --version          Show the version
`;
export function parseCliArgs(argv) {
    const parsed = {
        encode: false,
        decode: false,
        delimiter: ',',
        indent: '2',
        strict: true,
        stats: false,
        verbose: false,
        help: false,
        version: false,
    };
    const positionals = [];
    const unknown = [];
    let onlyPositionals = false;
    for (let index = 0; index < argv.length; index++) {
        const token = argv[index];
        if (onlyPositionals || token === '-' || !token.startsWith('-')) {
            positionals.push(token);
            continue;
        }
        if (token === '--') {
            onlyPositionals = true;
            continue;
        }
        index = token.startsWith('--')
            ? readLongOption(token, argv, index, parsed, unknown)
            : readShortCluster(token, argv, index, parsed, unknown);
    }
    if (positionals.length > 0)
        parsed.input = positionals[0];
    for (const surplus of positionals.slice(1))
        unknown.push(JSON.stringify(surplus));
    if (unknown.length > 0) {
        throw new CliError(`Unknown argument(s): ${[...new Set(unknown)].join(', ')} – see --help`);
    }
    return parsed;
}
/** Returns the index of the last argv token this option consumed. */
function readLongOption(token, argv, index, parsed, unknown) {
    const equals = token.indexOf('=');
    const name = equals === -1 ? token.slice(2) : token.slice(2, equals);
    const inlineValue = equals === -1 ? undefined : token.slice(equals + 1);
    const negated = !BY_NAME.has(name) && name.startsWith('no-');
    const option = BY_NAME.get(negated ? name.slice(3) : name);
    if (!option) {
        unknown.push(`--${name}`);
        return index;
    }
    if (option.kind === 'boolean') {
        assign(parsed, option.name, !negated && inlineValue !== 'false' && inlineValue !== '0');
        return index;
    }
    if (inlineValue !== undefined) {
        assign(parsed, option.name, inlineValue);
        return index;
    }
    assign(parsed, option.name, index + 1 < argv.length ? argv[index + 1] : '');
    return index + 1 < argv.length ? index + 1 : index;
}
/** Reads `-e`, `-o value`, `-ovalue`, and clusters such as `-ed`. */
function readShortCluster(token, argv, index, parsed, unknown) {
    const cluster = token.slice(1);
    for (let position = 0; position < cluster.length; position++) {
        const letter = cluster[position];
        const option = BY_ALIAS.get(letter);
        if (!option) {
            unknown.push(`-${letter}`);
            continue;
        }
        if (option.kind === 'boolean') {
            assign(parsed, option.name, true);
            continue;
        }
        const rest = cluster.slice(position + 1);
        if (rest !== '') {
            assign(parsed, option.name, rest.startsWith('=') ? rest.slice(1) : rest);
            return index;
        }
        assign(parsed, option.name, index + 1 < argv.length ? argv[index + 1] : '');
        return index + 1 < argv.length ? index + 1 : index;
    }
    return index;
}
function assign(parsed, name, value) {
    ;
    parsed[name] = value;
}
