# tq language reference

`tq` implements a growing, practical subset of jq's filter language. The
vendored compatibility corpus in `tests/corpus/tq/parity/` is the executable
compatibility contract; deliberate differences are recorded in
[`tq-jq-parity.md`](tq-jq-parity.md).

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

The jq aliases `todateiso8601` and `fromdateiso8601`, and the host-local
`localtime` builtin, remain deferred. They report tq's normal
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
