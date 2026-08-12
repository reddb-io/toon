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
cases should land together.

| Corpus case | tq behavior | jq 1.7.1 behavior | Rationale |
| --- | --- | --- | --- |
| `divergence-string-path-on-array` | `.x` over an array yields `null`. | Errors because an array cannot be indexed by a string. | tq treats an inapplicable named path like a missing path. |
| `divergence-index-on-object` | `.[0]` over an object yields `null`. | Errors because an object cannot be indexed by a number. | tq treats an inapplicable numeric path like a missing path. |
| `divergence-iteration-on-object` | `.[]` over an object yields `null`. | Streams the object's values. | tq iteration is deliberately array-only. |
| `divergence-number-has-on-object` | `has(1)` stringifies the key and returns `false`. | Errors on a numeric object key. | tq uses the same string-key lookup model as TOON objects. |
| `divergence-number-has-on-numeric-key` | `has(1)` stringifies the key and returns `true` for key `"1"`. | Errors on a numeric object key. | tq uses the same string-key lookup model as TOON objects. |
| `types-toarray-wraps-scalar` | Wraps a scalar in a one-element array. | `toarray/0` is not defined. | tq provides the requested scalar-to-array coercion. |
| `types-toarray-keeps-array` | Returns an array unchanged. | `toarray/0` is not defined. | tq provides the requested scalar-to-array coercion. |
| `types-toarray-normalizes-generated-numbers` | Wraps the generated value and emits jq-compatible JSON. | `toarray/0` is not defined. | tq provides the requested scalar-to-array coercion. |
| `divergence-chainable-comparison` | `1 < 2 < 3` evaluates left-associatively to `true`. | Rejects chained comparisons as a syntax error. | tq retains its established accepts-more comparison grammar. |
| `operators-alternative-suppresses-error` | `true \| length // 0` produces `0`. | Propagates the `length` type error. | tq's alternative contract suppresses errors raised by its left-hand filter. |
| `strings-trimstr` | Removes the requested affix from both ends. | `trimstr/1` is not defined. | tq provides the requested newer jq string surface while the corpus remains pinned to jq 1.7.1. |
| `strings-trim` | Removes leading and trailing Unicode whitespace. | `trim/0` is not defined. | tq provides the requested newer jq string surface while the corpus remains pinned to jq 1.7.1. |
| `strings-deferred-utf8bytelength-is-clear` | Reports an unsupported identifier. | Returns the string's UTF-8 byte length. | The themed slice keeps deferred string builtins explicit instead of silently accepting them. |

Array `to_entries` is explicitly not a divergence: jq 1.7.1 and tq both emit
zero-based numeric keys. Its ordinary parity pin is
`compatibility-array-to-entries`.
