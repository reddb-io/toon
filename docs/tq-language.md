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
