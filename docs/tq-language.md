# tq language reference

`tq` implements a growing, practical subset of jq's filter language. This page
is the normative reference for that subset: the precedence ladder every filter
is parsed against, the constructs the parser accepts, a catalog of every
builtin with its support status, and the table of deliberate differences from
jq.

The vendored compatibility corpus in `tests/corpus/tq/parity/` is the
executable compatibility contract, and [`tq-jq-parity.md`](tq-jq-parity.md) is
the divergence ledger this page's final table is drawn from. The `docs` test
target keeps the three in step: it fails when the catalog and the builtin
registry disagree, or when the divergence table, the ledger, and the corpus
stop naming the same cases.

## Support status

Every builtin in the catalog below carries one of three statuses.

| Status | Meaning |
| --- | --- |
| supported | Implemented and pinned by the parity corpus. |
| deferred | A jq 1.7.1 name tq does not implement yet. Calling it reports `unsupported identifier`, never a silent approximation. A later slice may ship it. |
| never | Deliberately out of scope. It reports the same diagnostic, and no slice will ship it, because the behavior contradicts a contract tq holds — a host timezone or locale, or a module search path. |

`supported` means shipped in the current release line rather than a per-builtin
version: the language surface was built slice by slice and the
[CHANGELOG](../CHANGELOG.md) records which slice added each group. A `deferred`
or `never` name is not a gap in the parity corpus — the corpus pins the
diagnostic, so the status is executable rather than aspirational.

## Precedence

A filter is parsed against this ladder, loosest binding first. Parentheses
override it everywhere.

| Level | Forms | Associativity |
| --- | --- | --- |
| 1 | `def name: body;`, `f as $x \| body`, `f \| g` | Right |
| 2 | `f, g` | Left |
| 3 | `f // g` | Left |
| 4 | `=`, `\|=`, `+=`, `-=`, `*=`, `/=`, `%=`, `//=` | None |
| 5 | `or` | Left |
| 6 | `and` | Left |
| 7 | `==`, `!=`, `<`, `<=`, `>`, `>=` | Left |
| 8 | `+`, `-` | Left |
| 9 | `*`, `/`, `%` | Left |
| 10 | Prefix `-` | Right |
| 11 | Postfix `.foo`, `[e]`, `[]`, `[a:b]`, `?` | Left |
| 12 | `.`, `..`, literals, `$x`, `[…]`, `{…}`, `@format`, `(…)`, a call | — |

Four consequences are worth stating outright, because they are where a filter
copied from a jq script most often reparses:

- A pipe is looser than a comma, so `.a, .b | length` pipes both into `length`.
- `as` takes everything after its `|` as the body, so
  `.[] as $x | $x, 1` binds over the whole comma list.
- Assignment sits between `//` and `or`, so `.a = .b // 5` groups as
  `(.a = .b) // 5` while `.a = .b or false` groups as `.a = (.b or false)`.
- Assignment is non-associative: `.a = .b = 1` is a syntax error rather than a
  chain, exactly as in jq.

Level 7 is a documented divergence. jq makes comparison non-associative and
rejects `1 < 2 < 3`; tq parses it left-associatively, so it evaluates to
`(1 < 2) < 3` and produces `true`. The ledger row is
`divergence-chainable-comparison`.

## Control flow

`if cond then f end` and `if cond then f elif cond then g else h end` both
parse; the `end` is required, and an absent `else` behaves as `else .`. The
condition is truthy for everything except `false` and `null`.

`try f catch g` runs `g` with the error as its input; `f?` is `try f` with no
handler, and both parse their operands at the assignment level, so
`try .a catch .b | length` pipes the result of the whole `try` into `length`.
A halt is not an error, so neither form recovers from it.

`reduce f as $x (init; update)` folds `f`'s values into one, and
`foreach f as $x (init; update)` emits the accumulator after each step. Both
accept the destructuring patterns `as` accepts — `$name`, `[…]`, and `{…}`,
nested freely — and both parse their generator at the comma level, so
`reduce .a, .b as $x (…)` folds over both.

`empty` produces nothing, `error` and `error(message)` raise, and `env` and
`$ENV` are the process environment as an object. Variables bound by `--arg`
and `--argjson` are in scope as `$name`, and `$ARGS` carries them as
`{"positional": […], "named": {…}}`.

jq's `label $out | … | break $out`, its alternative destructuring operator
`?//`, and its module directives `import` and `include` are not accepted;
they are reported as ordinary syntax errors.

## User-defined functions

`def name: body;` and `def name(a; $b): body;` define a filter over everything
that follows the semicolon, with jq's scoping:

- A bare parameter is a filter, evaluated in the caller's scope every time the
  body names it, so `def twice(f): f | f; twice(. + 1)` composes the argument.
- A `$name` parameter also binds one value at a time, exactly as jq's
  `def f(a): a as $a | …` sugar does, and stays callable as the filter `a`.
- A body sees the definition itself, so it can recurse, plus everything defined
  and bound before it. A later definition of the same name and arity shadows the
  earlier one for later callers only, and a definition shadows a builtin of the
  same name and arity.

Evaluation nesting is bounded. tq evaluates a nested filter as a nested call, so
a runaway definition such as `def f: f; f` reports
`exceeded the maximum filter recursion depth` and stops the query instead of
exhausting the process stack. jq has no such limit, so a filter whose recursion
is deeper than the budget is a deliberate divergence: it fails in tq where jq
grinds on.

## The path layer

Every filter says what it produces. A *path expression* also says where each
produced value lived, and `path(f)` asks for that second answer:

```console
$ echo '{"users":[{"name":"Ada"}]}' | tq -p json -o json -c 'path(.users[0].name)'
["users",0,"name"]
```

The forms that carry a location are identity, field access, indexing, slicing,
iteration, pipes, commas, `select`, `if`, `//`, `getpath`, `try`/`?`, `empty`,
variable bindings, recursive descent, and any user definition whose body is
itself made of them. Anything else — arithmetic, `length`, a literal — has no
location, so `path()` reports jq's `Invalid path expression with result …`
naming the value that was computed instead.

The builtins built on that machinery are:

| Builtin | Result |
| --- | --- |
| `path(f)` | The path `f` selects, as an array of components. |
| `paths` | Every path below the input; the root's empty path is excluded. |
| `paths(f)` | Those paths whose value satisfies `f`. |
| `leaf_paths` | Those paths whose value is a scalar. |
| `getpath(p)` | The value at path `p`, or `null` where the path is missing. |
| `setpath(p; v)` | The input with `v` written at path `p`. |
| `delpaths(ps)` | The input with every path in `ps` removed. |
| `del(f)` | The input with everything `f` selects removed. |
| `..`, `recurse` | The input and every value below it. |
| `recurse(f)` | The input, then `f` applied repeatedly until it produces nothing. |
| `recurse(f; cond)` | The same, descending only through values satisfying `cond`. |
| `pick(f)` | A value keeping only the paths `f` selects. |
| `tostream` | The input as jq's `[path, leaf]` and `[path]` event stream. |
| `fromstream(f)` | The values that event stream rebuilds. |

A slice appears in a path as jq spells it, `{"start": …, "end": …}`, with
`null` for an open end.

Reads stay lazy. `path`, `getpath`, and a plain field or index query walk the
codec's accessors, so naming one row of a large tabular array decodes that row
and no other (ADR 0002). Writes are the sanctioned exception: `setpath`,
`delpaths`, and `del` materialise the tabular array they touch into a list
array, because a row-backed array cannot represent an edited row. Arrays they
do not touch stay undecoded.

One consequence of tq's lenient named-path model is worth stating: `.a` yields
`null` on a non-object rather than failing, so `path(.a.b)` reaches through a
scalar where jq stops, and a `recurse(.a?)` that terminates in jq keeps
descending in tq until the recursion budget stops it. Reach for `.[]?`, whose
iteration does fail on a scalar, when you want jq's termination.

## Assignment

The assignment family edits a document in place. Every operator is sugar over
the path layer: it locates the paths its left-hand side selects, then writes at
each one with `setpath`.

| Operator | Writes at every selected path |
| --- | --- |
| `p = v` | `v`. |
| `p \|= f` | The first value `f` produces from the value already there. |
| `p += v`, `p -= v`, `p *= v`, `p /= v`, `p %= v` | That operator applied to the value already there and `v`. |
| `p //= v` | `v`, but only where the value already there is `false` or `null`. |

```console
$ echo '{"users":[{"name":"Ada"}]}' | tq -p json -o json -c '.users[0].name = "Grace"'
{"users":[{"name":"Grace"}]}
```

The left-hand side is a path expression, so everything `path()` accepts works
there — a multi-path form such as `(.a,.b) = 1` writes at both, and
`(.[]|select(.score < 0)) = 0` writes at each match. A left-hand side that
computes a value instead reports `Invalid path expression`.

Where the right-hand side is evaluated differs by operator, exactly as in jq.
`|=` runs its filter at every selected value, so it always yields one edited
document. Every other operator evaluates its right-hand side once against the
whole input, which is why `.a += .b` reads `.b` from the document rather than
from `.a`, and why a generator there yields one edited document per value:
`.a = (1,2)` produces two.

A missing path is created rather than rejected, so `.a.b = 1` on `{}` builds
the objects on the way down and `.a += 1` on `{}` starts from `null`.

`|=` is the one operator whose update can produce nothing, and jq gives that a
meaning: the path is deleted. `.a |= empty` removes `.a`, and
`.[] |= select(. > 1)` keeps only the matching elements. Deletions are
collected and applied together at the end, so removing one element never
shifts a path still to be visited — `[1,2,3,4,5] | .[] |= empty` empties the
array rather than deleting every second element.

Assignment is non-associative, as in jq: `.a = .b = 1` is rejected rather than
grouped. It binds tighter than `//`, so `.a += 1 // 5` increments `.a`.

The paths are located once, against the document that entered the assignment,
and the laziness rules above apply unchanged: assigning into one row of a
tabular array materialises that array and leaves every other one undecoded.

## Strings, formats, and JSON conversions

A string literal interpolates: `"\(f)"` splices what `f` produces into the
surrounding text.

```console
$ echo '{"name":"Ada"}' | tq -p json -o json -c '"hello \(.name)"'
"hello Ada"
```

Interpolation is sugar over concatenation — `"a\(f)b"` means
`f as $x | "a" + ($x|@text) + "b"` — so a filter that produces several values
produces several strings, one per combination, and one that produces none
produces no string at all. As in jq, the last `\(…)` in a string varies
slowest: `["\(1,2)-\(3,4)"]` is `["1-3","2-3","1-4","2-4"]`.

A `@format` name applies a format to the input:

| Format | Result |
| --- | --- |
| `@text` | The input as text: a string unchanged, anything else as JSON. |
| `@json` | The input as compact JSON. |
| `@csv` | An array as one CSV row; strings are quoted and their quotes doubled. |
| `@tsv` | An array as one TSV row; tab, newline, return, and backslash are escaped. |
| `@base64` | The input's text, base64-encoded. |
| `@base64d` | The input's text, base64-decoded. |
| `@uri` | The input's text, percent-encoded. |
| `@html` | The input's text with `<`, `>`, `&`, `'`, and `"` as entities. |
| `@sh` | The input as shell words; a string is single-quoted, an array is a word list. |

The same name in front of a string literal applies the format to every
interpolation in it and leaves the literal text alone, which is what makes the
format worth having:

```console
$ echo '{"q":"a b&c"}' | tq -p json -o json -c '@uri "https://example.com/?q=\(.q)"'
"https://example.com/?q=a%20b%26c"
```

`@csv` and `@tsv` take an array of scalars: a nested array or object has no
cell spelling, and neither does a non-array input. `@sh` refuses a nested array
or object for the same reason. `@base64d` rejects a character outside the
base64 alphabet, and a final group holding a single character, which carries
too few bits to complete a byte.

The conversions between values and their JSON text are:

| Builtin | Result |
| --- | --- |
| `tostring` | The input as text, exactly as `@text`. |
| `tonumber` | A number unchanged; a string parsed as one. |
| `tojson` | The input as compact JSON text. |
| `fromjson` | A string parsed as JSON. |

`tonumber` parses its string as JSON and keeps the result only when it is a
number, so `"[1]"` reports that it cannot be parsed as a number while `"abc"`
reports an invalid numeric literal.

Object and pattern keys stay literal. `{"\(.a)": 1}` names one field at parse
time in jq; tq reports the interpolated key instead of building it at run time.

## UTC time builtins

Time handling is UTC-only and does not read the process timezone or locale.
Timestamps are Unix seconds, and broken-down times use jq's eight-element
layout:

```text
[year, month_from_zero, day, hour, minute, second, weekday_from_sunday, year_day_from_zero]
```

The supported names are:

| Builtin | Result |
| --- | --- |
| `now` | Current Unix timestamp. |
| `gmtime` | Convert a timestamp to a broken-down UTC array. |
| `mktime` | Convert a broken-down UTC array to a timestamp, normalizing out-of-range calendar and clock components. |
| `todate` | Format a timestamp as `YYYY-MM-DDTHH:MM:SSZ`. |
| `fromdate` | Parse that same ISO-shaped UTC form to a timestamp. |
| `strftime(format)` | Format a timestamp or broken-down time with the portable directives below. |
| `strptime(format)` | Parse the documented numeric subset below to a broken-down time. |

`strftime` interprets `%Y`, `%y`, `%m`, `%d`, `%e`, `%H`, `%M`, `%S`, `%j`,
`%w`, `%u`, `%F`, `%R`, `%T`, and `%%`. Other directives are emitted
literally, matching jq's behavior for an unknown directive. `%z` and `%Z`
instead fail clearly because their jq results depend on host timezone data,
which is outside tq's UTC-only contract.

`strptime` accepts literal separators plus `%Y`, `%m`, `%d`, `%H`, `%M`,
`%S`, and `%%`. Year, month, and day are required; the other fields default to
zero. Numeric fields other than the year use two digits. Locale-dependent
names, week numbering, timezone offsets, and locale date/time composites are
not parsed.

The jq aliases `todateiso8601` and `fromdateiso8601` remain deferred: they are
spellings of `todate` and `fromdate` that no slice has wired up yet. The
host-local builtins `localtime` and `strflocaltime` are a different case — they
read the process timezone, which the UTC-only contract excludes, so they will
not be shipped at all. All of them report tq's normal
`unsupported identifier` diagnostic rather than silently applying local-time
semantics.

## Tracing, halting, and reading further input

These builtins talk to the run itself rather than to the value flowing through
it: what it traces, what it exits with, and what it has not read yet.

| Builtin | Effect |
| --- | --- |
| `debug` | Writes `["DEBUG:", <input>]` to stderr and passes the input on. |
| `debug(msgs)` | Writes one such line per value `msgs` produces, then passes the input on. |
| `stderr` | Writes the input to stderr — a string as-is, anything else as compact JSON, neither followed by a newline — and passes it on. |
| `halt` | Ends the run immediately with status 0. |
| `halt_error` | Writes the input to stderr and ends the run with status 5. |
| `halt_error(status)` | The same, with the status given; only its low eight bits reach the shell, so `halt_error(300)` exits 44. |
| `input` | The next document, or an error once the stream is exhausted. |
| `inputs` | Every document not read yet. |

`halt_error` writes a string payload as-is and anything else as compact JSON
followed by a newline, which is how jq distinguishes a prepared message from a
dumped value.

A halt is the end of the program, not an error, so `try`, `?` and `//` re-raise
it rather than recovering from it: `try halt_error catch "caught"` still exits
5. What tq does not reproduce is jq's streaming: tq evaluates a document to
completion before writing anything, so a halt cancels the output of the
document it happened in — `1,2,halt` writes nothing where jq writes `1` and
`2`. Documents already written stay written, so a stream that halts at its
third row keeps the first two.

`input` and `inputs` read from the same cursor the run is already walking,
rather than from a copy of it:

```console
$ printf '[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n' | tq -p toonl -o json -c '[.name,input.name]'
["Ada","Linus"]
```

The stream is never slurped to make that work, so a row no filter asks for is
never decoded. Under `-n` the filter runs once against `null` and every row is
left for `inputs` to draw, which is jq's `-n '[inputs]'` idiom. `--slurp` is
the opposite end: it has already consumed the stream into the document, so
`inputs` finds nothing. tq evaluates a filter to completion rather than
streaming it, so `inputs` draws every remaining document when it runs.

Where the input is a single document — JSON, TOON, YAML, or XML — there is no
next document, so `inputs` is empty and `input` reports `No more inputs`.

`input_line_number` is deferred: tq does not carry byte positions through its
decoders, so it reports the usual `unsupported identifier` diagnostic rather
than an approximation.

## Builtin catalog

Every name the builtin registry dispatches appears below as `supported`,
grouped by the theme that shipped it. Each group also lists the jq 1.7.1 names
in that theme that tq does not dispatch, so a filter that fails to run can be
told apart from one that is spelled wrong. `empty`, `env`, `$ENV`, `$ARGS`,
and the `--arg` variables are language constructs rather than registry
entries; they are described under [Control flow](#control-flow).

### Core

| Builtin | Status | Result |
| --- | --- | --- |
| `add` | supported | The input's values summed, concatenated, or merged, and `null` for an empty input. |
| `not` | supported | The logical negation of the input's truthiness. |
| `select(f)` | supported | The input where `f` is truthy, and nothing where it is not. |
| `error`, `error(message)` | supported | Raises the input, or the given value, as an error. |
| `add(f)` | deferred | jq's generator form; use `[f] \| add`. |

### Types and selectors

| Builtin | Status | Result |
| --- | --- | --- |
| `type` | supported | The input's type name. |
| `length` | supported | Size for a string, array, or object; magnitude for a number; `0` for `null`. |
| `arrays`, `booleans`, `iterables`, `nulls`, `numbers`, `objects`, `scalars`, `strings`, `values` | supported | The input when it has that shape, and nothing otherwise. |
| `infinite`, `nan` | supported | The non-finite constants. |
| `isinfinite`, `isnan` | supported | Whether the input is that constant. |
| `toarray` | supported | The input as an array, wrapping a non-array. A tq addition; ledgered as `types-toarray-*`. |
| `utf8bytelength` | deferred | Ledgered as `strings-deferred-utf8bytelength-is-clear`. |
| `have_literal_numbers`, `have_decnum` | never | Probes for how jq itself was built; they describe nothing about tq. |

### Arrays and generators

| Builtin | Status | Result |
| --- | --- | --- |
| `map(f)` | supported | `f` applied to every value of the input. |
| `sort`, `sort_by(f)` | supported | The array ordered by value, or by `f`'s values. |
| `group_by(f)`, `unique`, `unique_by(f)` | supported | Grouping and deduplication under the same ordering. |
| `min`, `max`, `min_by(f)`, `max_by(f)` | supported | The extreme element, and `null` for an empty array. |
| `reverse`, `flatten`, `flatten(depth)`, `transpose` | supported | Structural rearrangements of an array. |
| `contains(v)`, `inside(v)` | supported | Containment in either direction. |
| `index(v)`, `rindex(v)`, `indices(v)` | supported | The first, last, and all offsets of `v`. |
| `first`, `last`, `nth(n)` | supported | An element of the input array. |
| `first(f)`, `last(f)`, `nth(n; f)` | supported | An element of what `f` produces. |
| `limit(n; f)` | supported | The first `n` values `f` produces. |
| `all`, `any`, `all(f)`, `any(f)`, `all(gen; f)`, `any(gen; f)` | supported | Whether every or some value is truthy. |
| `range(hi)`, `range(lo; hi)`, `range(lo; hi; step)` | supported | A bounded numeric sequence. |
| `until(cond; update)`, `while(cond; update)` | supported | Iteration that stops on `cond`. |
| `range` | deferred | The unbounded form. Ledgered as `stream-unbounded-range-unsupported`. |
| `repeat(f)`, `repeat(f; n)` | deferred | Ledgered as `stream-repeat-deferred`. |
| `combinations`, `combinations(n)` | deferred | Not yet shipped by an array slice. |
| `IN`, `INDEX`, `GROUP_BY`, `UNIQUE_BY`, `ANY`, `ALL` | deferred | jq's SQL-style helpers; the lowercase primitives they are built on are supported. |

### Objects

| Builtin | Status | Result |
| --- | --- | --- |
| `keys`, `keys_unsorted` | supported | The field names, sorted or in field order. |
| `has(key)` | supported | Whether the field or index exists. Ledgered as `divergence-number-has-on-*` for numeric keys. |
| `to_entries`, `from_entries`, `with_entries(f)` | supported | The object as `{key, value}` records and back. |
| `walk(f)` | supported | `f` applied bottom-up to every value below the input. |
| `map_values(f)` | deferred | Use `.[] \|= f`, which the assignment layer already implements. |

### Strings

| Builtin | Status | Result |
| --- | --- | --- |
| `ascii_downcase`, `ascii_upcase` | supported | ASCII-only case folding. |
| `startswith(s)`, `endswith(s)` | supported | Affix tests. |
| `ltrimstr(s)`, `rtrimstr(s)`, `trimstr(s)` | supported | The string with that affix removed from one or both ends. `trimstr` is ledgered as `strings-trimstr`. |
| `trim` | supported | Leading and trailing Unicode whitespace removed. Ledgered as `strings-trim`. |
| `split(sep)` | supported | The string cut on a literal separator. |
| `join(sep)` | supported | An array joined into one string. |
| `explode`, `implode` | supported | The string as codepoints and back. |
| `ascii(n)` | deferred | The single-codepoint constructor; `[n] \| implode` is the equivalent. |

### Regular expressions

| Builtin | Status | Result |
| --- | --- | --- |
| `test(re)`, `test(re; flags)` | supported | Whether the pattern matches. |
| `match(re)`, `match(re; flags)` | supported | One match object per match. |
| `capture(re)`, `capture(re; flags)` | supported | Named captures as an object. |
| `scan(re)`, `scan(re; flags)` | supported | Every match, with its captures where the pattern has them. |
| `split(re; flags)`, `splits(re)`, `splits(re; flags)` | supported | The string cut on a pattern, as an array or a stream. |
| `sub(re; s)`, `sub(re; s; flags)`, `gsub(re; s)`, `gsub(re; s; flags)` | supported | The first or every match replaced. |

### Math

| Builtin | Status | Result |
| --- | --- | --- |
| `floor`, `ceil`, `round`, `trunc` | supported | Rounding toward the named direction. |
| `abs`, `fabs` | supported | Magnitude. |
| `sqrt`, `pow(x; y)`, `exp`, `log`, `log2`, `log10` | supported | Powers, roots, and logarithms. |
| `significand` | supported | The mantissa of the input's binary representation. |
| `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2(y; x)`, and the hyperbolic family | deferred | Ledgered as `math-unsupported-sin`. |
| `cbrt`, `exp2`, `exp10`, `expm1`, `log1p`, `logb`, `frexp`, `modf`, `ldexp(m; e)`, `drem(a; b)`, `nearbyint`, `hypot(a; b)`, `fmin(a; b)`, `fmax(a; b)`, `fma(a; b; c)`, `lgamma`, `tgamma` | deferred | The rest of jq's C math surface; the portable subset shipped first. |

### Paths and streams

The semantics of this group are described under
[The path layer](#the-path-layer).

| Builtin | Status | Result |
| --- | --- | --- |
| `path(f)`, `paths`, `paths(f)`, `leaf_paths` | supported | Locations rather than values. `leaf_paths` is ledgered as `paths-leaf-paths-keeps-scalars`. |
| `getpath(p)`, `setpath(p; v)`, `delpaths(ps)`, `del(f)`, `pick(f)` | supported | Reads, writes, and deletions addressed by path. |
| `recurse`, `recurse(f)`, `recurse(f; cond)` | supported | Descent through the document. `..` is `recurse`. |
| `tostream`, `fromstream(f)` | supported | The document as a path/leaf event stream and back. |
| `truncate_stream(f)` | deferred | The depth-shifting stream helper. |

### Time

Semantics, the eight-element broken-down layout, and the supported `strftime`
and `strptime` directives are described under
[UTC time builtins](#utc-time-builtins).

| Builtin | Status | Result |
| --- | --- | --- |
| `now` | supported | The current Unix timestamp. |
| `gmtime`, `mktime` | supported | Timestamp and broken-down UTC time, in both directions. |
| `todate`, `fromdate` | supported | The `YYYY-MM-DDTHH:MM:SSZ` form, in both directions. |
| `strftime(format)`, `strptime(format)` | supported | Formatting and parsing over the documented directive subset. |
| `todateiso8601`, `fromdateiso8601` | deferred | jq's aliases for `todate` and `fromdate`. |
| `localtime`, `strflocaltime(format)`, and the `%z` and `%Z` directives | never | Their results depend on the host timezone, which tq's UTC-only contract excludes. |

### Formats and conversions

Format semantics are described under
[Strings, formats, and JSON conversions](#strings-formats-and-json-conversions).

| Builtin | Status | Result |
| --- | --- | --- |
| `@text`, `@json` | supported | The input as text or as compact JSON. |
| `@csv`, `@tsv` | supported | An array of scalars as one delimited row. |
| `@base64`, `@base64d`, `@uri`, `@html`, `@sh` | supported | Encodings of the input's text. |
| `tostring`, `tonumber`, `tojson`, `fromjson` | supported | Conversions between values and their text. |
| `@base32`, `@base32d` | deferred | The base32 pair; base64 shipped first. |

### Runtime

Semantics are described under
[Tracing, halting, and reading further input](#tracing-halting-and-reading-further-input).

| Builtin | Status | Result |
| --- | --- | --- |
| `debug`, `debug(msgs)`, `stderr` | supported | Diagnostics on stderr; the input passes through. |
| `halt`, `halt_error`, `halt_error(status)` | supported | Ends the run. Ledgered as `misc-halt-discards-the-output-still-pending`. |
| `input`, `inputs` | supported | Documents not read yet. `input` at the end of the stream is ledgered as `misc-input-without-a-next-document`. |
| `input_line_number` | deferred | Ledgered as `misc-deferred-input-line-number-is-clear`. |
| `input_filename` | deferred | tq takes an optional file argument, so the name has a meaning; it is simply not wired through yet. |
| `builtins` | deferred | This catalog is the checked-in equivalent. |
| `$__loc__` | deferred | tq does not carry source positions into evaluation. |
| `import`, `include`, `modulemeta`, `get_search_list`, `set_search_list` | never | tq is a single binary with no module search path. |

## Where tq differs from jq

Each row is a deliberate difference with a case in the parity corpus. The
rationale and jq's exact behavior live in the divergence ledger in
[`tq-jq-parity.md`](tq-jq-parity.md); this table names the same ids in the
same order, and the `docs` test target fails if the two ever drift apart.

| Ledger id | Difference |
| --- | --- |
| `divergence-string-path-on-array` | `.x` over an array yields `null` instead of failing. |
| `divergence-index-on-object` | `.[0]` over an object yields `null` instead of failing. |
| `divergence-number-has-on-object` | `has(1)` stringifies the key instead of failing. |
| `divergence-number-has-on-numeric-key` | The same stringified lookup finds the field named `"1"`. |
| `types-toarray-wraps-scalar` | `toarray` exists and wraps a scalar. |
| `types-toarray-keeps-array` | `toarray` leaves an array unchanged. |
| `types-toarray-normalizes-generated-numbers` | `toarray` emits jq-compatible JSON for a generated number. |
| `divergence-chainable-comparison` | `1 < 2 < 3` parses left-associatively instead of being rejected. |
| `operators-alternative-suppresses-error` | `//` suppresses an error raised by its left-hand filter. |
| `errors-alternative-catches-structured-error` | `//` suppresses it even when the error value is not a string. |
| `strings-trimstr` | `trimstr` exists. |
| `strings-trim` | `trim` exists. |
| `strings-deferred-utf8bytelength-is-clear` | `utf8bytelength` is deferred and says so. |
| `paths-leaf-paths-keeps-scalars` | `leaf_paths` exists, keeping jq 1.6's spelling. |
| `paths-field-through-scalar` | `path(.a.b)` reaches through a scalar. |
| `interp-object-key-is-deferred` | An interpolated object key is reported rather than built. |
| `paths-try-handler-cannot-produce-a-path` | A `try` in path mode reports the body's error, not the handler's. |
| `math-unsupported-sin` | `sin` is deferred and says so. |
| `stream-repeat-deferred` | `repeat` is deferred and says so. |
| `stream-unbounded-range-unsupported` | `range` with no arguments is deferred and says so. |
| `misc-input-without-a-next-document` | An exhausted `input` reports `No more inputs`. |
| `misc-halt-discards-the-output-still-pending` | A halt cancels the output of the document it happened in. |
| `misc-deferred-input-line-number-is-clear` | `input_line_number` is deferred and says so. |
