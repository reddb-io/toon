# tq and jq parity

The jq-parity corpus lives in `tests/corpus/tq/parity/`. It is the executable
record of the jq-compatible filter behavior implemented by tq. The `parity`
test target always checks tq against the vendored expectations, so its normal
CI path is hermetic and does not require jq.

The same target has a corpus validator. It probes `jq --version` and replays
the corpus only when the result is exactly `jq-1.7.1`. A missing binary or any
other version produces a warning and skips validation; filters are never sent
to an unpinned jq.

## Fixture format

Each `.cases` file groups one theme. Cases are separated by a blank line and
each field occupies one line. Field values are JSON strings so filters, input,
and output can contain quoting and newlines without making the format
ambiguous.

```text
case: "unique-case-name"
filter: ".items|length"
input: "{\"items\":[1,2]}"
output: "2\n"
```

Every case has `case`, `filter`, `input`, and exactly one of `output` or
`error`. `error` is a required stderr substring for a failing tq invocation.
Case names are unique across the complete corpus.

A deliberate tq-ism also has `divergence` and `jq-output`. The latter is jq's
exact compact stdout, or `error: ` followed by a required jq stderr substring.
The validator checks both the documented jq result and that tq still differs.

## Divergence ledger

These differences are intentional. Changes to this table and their corpus
cases should land together: the `docs` test target asserts the table names
exactly the corpus cases that carry a `divergence` field, and that
[`tq-language.md`](tq-language.md) repeats the same ids in the same order.

| Corpus case | tq behavior | jq 1.7.1 behavior | Rationale |
| --- | --- | --- | --- |
| `divergence-string-path-on-array` | `.x` over an array yields `null`. | Errors because an array cannot be indexed by a string. | tq treats an inapplicable named path like a missing path. |
| `divergence-index-on-object` | `.[0]` over an object yields `null`. | Errors because an object cannot be indexed by a number. | tq treats an inapplicable numeric path like a missing path. |
| `divergence-number-has-on-object` | `has(1)` stringifies the key and returns `false`. | Errors on a numeric object key. | tq uses the same string-key lookup model as TOON objects. |
| `divergence-number-has-on-numeric-key` | `has(1)` stringifies the key and returns `true` for key `"1"`. | Errors on a numeric object key. | tq uses the same string-key lookup model as TOON objects. |
| `types-toarray-wraps-scalar` | Wraps a scalar in a one-element array. | `toarray/0` is not defined. | tq provides the requested scalar-to-array coercion. |
| `types-toarray-keeps-array` | Returns an array unchanged. | `toarray/0` is not defined. | tq provides the requested scalar-to-array coercion. |
| `types-toarray-normalizes-generated-numbers` | Wraps the generated value and emits jq-compatible JSON. | `toarray/0` is not defined. | tq provides the requested scalar-to-array coercion. |
| `divergence-chainable-comparison` | `1 < 2 < 3` evaluates left-associatively to `true`. | Rejects chained comparisons as a syntax error. | tq retains its established accepts-more comparison grammar. |
| `operators-alternative-suppresses-error` | `true \| length // 0` produces `0`. | Propagates the `length` type error. | tq's alternative contract suppresses errors raised by its left-hand filter. |
| `errors-alternative-catches-structured-error` | `error({"ignored":true}) // 7` produces `7`. | Fails with `(not a string)` because the structured payload escapes the alternative. | The same alternative contract, applied to a non-string error value: tq suppresses what its left-hand filter raised rather than inspecting the payload. |
| `strings-trimstr` | Removes the requested affix from both ends. | `trimstr/1` is not defined. | tq provides the requested newer jq string surface while the corpus remains pinned to jq 1.7.1. |
| `strings-trim` | Removes leading and trailing Unicode whitespace. | `trim/0` is not defined. | tq provides the requested newer jq string surface while the corpus remains pinned to jq 1.7.1. |
| `strings-deferred-utf8bytelength-is-clear` | Reports an unsupported identifier. | Returns the string's UTF-8 byte length. | The themed slice keeps deferred string builtins explicit instead of silently accepting them. |
| `paths-leaf-paths-keeps-scalars` | `leaf_paths` lists the paths of every scalar. | `leaf_paths/0` is not defined. | tq keeps jq 1.6's spelling, which jq 1.7 removed; the path layer implements it directly. |
| `paths-field-through-scalar` | `path(.a.b)` reaches through a scalar and reports `["a","b"]`. | Errors because a number cannot be indexed by a string. | The same lenient named-path model as `divergence-string-path-on-array`, applied in path mode. |
| `interp-object-key-is-deferred` | Reports an interpolated object or pattern key. | Builds the key from the interpolation. | tq keys name one field before the query runs; dynamic keys wait for the object-construction slice. |
| `paths-try-handler-cannot-produce-a-path` | Reports the error the `try` body raised. | Reports that the `catch` handler is not a path expression. | Neither filter has a path; tq keeps the original diagnostic rather than describing the handler. |
| `math-unsupported-sin` | `sin` reports an unsupported identifier. | Returns the sine of the input. | The math slice shipped the portable subset; the C trigonometric family stays deferred rather than half-implemented. |
| `stream-repeat-deferred` | `repeat(f; n)` reports an unsupported identifier. | Reports that `repeat/2` is not defined, because jq only defines `repeat/1`. | tq names the deferred builtin rather than its arity: `repeat` waits until the evaluator can stop an unbounded generator. |
| `stream-unbounded-range-unsupported` | `range` with no arguments reports an unsupported identifier. | Reports that `range/0` is not defined. | Same eager-evaluator reason, and tq keeps its one unsupported-identifier wording instead of jq's undefined-function wording. |
| `misc-input-without-a-next-document` | Reports `No more inputs`. | Lets its internal `break` escape as the error message. | tq names the condition the filter hit instead of leaking an interpreter token. |
| `misc-halt-discards-the-output-still-pending` | `1,2,halt` writes nothing. | Writes `1` and `2` before halting. | tq evaluates a document to completion before writing anything, so a halt cancels that document's output; documents already written stay written. |
| `misc-deferred-input-line-number-is-clear` | Reports an unsupported identifier. | Returns the number of input lines read so far. | tq does not carry byte positions through its decoders, so the line counter is deferred rather than approximated. |

`format-tsv-rejects-nested-object` is explicitly not a divergence either: jq
reports an unformattable tsv cell with its csv wording, and tq repeats the
message rather than correcting it, because the message is what parity pins.

Array `to_entries` is explicitly not a divergence: jq 1.7.1 and tq both emit
zero-based numeric keys. Its ordinary parity pin is
`compatibility-array-to-entries`.

Object iteration is no longer a divergence either. `.[]` over an object once
yielded `null` and was ledgered as `divergence-iteration-on-object`; it now
streams the object's values and raises on `null` and scalars, exactly as jq
does, so the row and its case were retired together.

## The compatibility decision

`tq jq-check` answers, without running anything, whether tq can execute a jq
invocation with jq-compatible observable behavior. It exists for command
proxies that want to substitute tq for jq only where the substitution is
backed by this corpus.

```bash
tq jq-check [jq option]... [--] <filter>
```

The filter is the last argument, or the single argument after `--`. Nothing is
read from stdin and the filter is never evaluated, so a negative decision costs
no partial interpretation.

### The contract

A positive decision promises this, and only this:

> For every input on which jq 1.7.1 succeeds, `tq -p json -o json` with the
> same options and filter produces jq's exact output.

The `-p json -o json` transport is part of the promise, because tq's own
default is TOON. `jq-check` accepts `-p json` and `-o json` for that reason and
refuses any other transport.

The promise is deliberately silent about inputs on which jq 1.7.1 *fails*.
Several ledger rows above are cases where tq answers and jq raises — a named
path on an array, a numeric index on an object, `//` over an erroring left-hand
filter. Those keep a positive decision, and the ledger's `jq-output` column is
what proves each one is of that shape: the `compat` test target refuses any
ledger row whose recorded jq result is not an error unless `jq-check` also
refuses its filter.

Where tq would differ on an input jq accepts, the decision is always negative.

### Output

One JSON object on stdout. Exit `0` when compatible, `1` when not.

```json
{
  "jq_version": "1.7.1",
  "filter": "sin",
  "options": [],
  "compatible": false,
  "reasons": [
    { "kind": "unsupported-builtin", "detail": "`sin/0` is not implemented" }
  ]
}
```

`reasons` is empty exactly when `compatible` is `true`. Each `kind` is one of a
stable set; `detail` is prose and may be reworded.

| Reason kind | Meaning |
| --- | --- |
| `unsupported-option` | An option tq does not honor with jq-compatible behavior, including a non-JSON transport and tq's own options. |
| `unsupported-syntax` | The filter does not parse, so tq cannot run it at all. |
| `unsupported-builtin` | The filter names something the builtin registry does not dispatch at that arity. |
| `divergent-builtin` | The filter names a builtin the ledger above records. |
| `divergent-syntax` | The filter uses a construct jq 1.7.1 reads differently, or rejects. |

### How the decision is derived

Nothing here is a hand-maintained allowlist. Options are matched against the
one table `tq`'s own argument parser dispatches from, and calls are resolved
through the evaluator's builtin registry, where a ledger row is recorded on the
registry entry itself. A builtin added to the registry is classified the moment
it lands; a `Builtin::new(…).divergent(…)` entry is refused from then on.

Two decisions are conservative by construction, because the filter alone cannot
settle them:

- `has` with a literal numeric key is refused. It is jq-compatible over an
  array and the ledgered divergence over an object.
- A `def` shadowing a builtin is trusted: the decision follows the same
  resolution order evaluation does, so the definition is classified, not the
  builtin it hides.
