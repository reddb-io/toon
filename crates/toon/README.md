# reddb-io-toon

> **Attribution:** This is RedDB's Rust implementation of TOON - not the original project. The TOON format was created by Johann Schopplich; see the [official repo](https://github.com/toon-format/toon), [toon-format/spec](https://github.com/toon-format/spec), and [toonformat.dev](https://toonformat.dev) for the format spec and original project.

Rust parser, serializer, document model, encode extensions, and TOONL v0.2 stream utilities for TOON v3.3.

The crate emits canonical TOON v3.3 by default. The reddb-io encode extensions are always decoded but only encoded when requested with [`EncodeOptions`]. The extension specs live in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md), and TOONL is specified in [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md).

```toml
[dependencies]
reddb-io-toon = "0.8.0"
```

## Public Model

`Value` is the root enum: `Object(Document)`, `Array(Array)`, `String`, `Number`, `Bool`, and `Null`. `Document` is an ordered object model with parsed fields. `Array` stores either a normal `List(Vec<Value>)` or a `Tabular(TabularArray)` so table-shaped arrays can be decoded without immediately materializing every row into nested documents.

```rust
use reddb_io_toon::{Array, Document, EncodeOptions, ParseOptions, Value};

let value = Value::parse_toon("users[1]{id,name}:\n  1,Ada\n")?;
let document = Document::parse("users[1]{id,name}:\n  1,Ada\n")?;
let users = document.get("users").and_then(Value::as_array).expect("users array");

assert!(matches!(users, Array::Tabular(_)));
assert_eq!(value.to_canonical_toon(), "users[1]{id,name}:\n  1,Ada\n");
assert_eq!(
    value.to_toon_with_options(EncodeOptions::default()),
    value.to_canonical_toon()
);

let parsed = Value::parse_with_options(
    "a.b: 1\n",
    ParseOptions {
        expand_paths: true,
        ..ParseOptions::default()
    },
)?;
assert_eq!(parsed.to_json_string(true)?, r#"{"a":{"b":1}}"#);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Main entry points:

- `Value::parse_toon(input)` parses any TOON root value.
- `Value::parse_with_options(input, options)` accepts `indent`, `strict`, `expand_paths`, and `max_depth`.
- `Document::parse(input)` and `Document::parse_with_options(input, options)` require an object root.
- `Value::from_json_str(input)` and `Value::from_json_value(value)` convert JSON into the same model.
- `to_canonical_toon()` emits canonical v3.3.
- `to_toon_with_options(options)` emits canonical v3.3 plus any requested extensions.
- `try_to_canonical_toon()` and `try_to_toon_with_options(options)` return `EncodeError`.
- `to_json_value()` and `to_json_string(compact)` convert back to `serde_json`.

## Parse Options

Strict mode is on by default and enforces the v3.3 error checklist. Set `strict: false` only when accepting legacy recovery behavior is intentional.

```rust
use reddb_io_toon::{ParseOptions, Value};

let input = "a: 1\na: 2\n";
assert!(Value::parse_toon(input).is_err());

let recovered = Value::parse_with_options(
    input,
    ParseOptions {
        strict: false,
        ..ParseOptions::default()
    },
)?;
assert_eq!(recovered.to_json_string(true)?, r#"{"a":2}"#);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`max_depth` protects decoders and fallible encoders from untrusted nesting. `0` disables the guard for trusted input.

```rust
use reddb_io_toon::{ParseOptions, Value};

let error = Value::parse_with_options(
    "a:\n  b:\n    c: 1\n",
    ParseOptions {
        max_depth: 1,
        ..ParseOptions::default()
    },
)
.expect_err("depth guard rejects the third level");

assert_eq!(error.line(), 3);
assert!(error.to_string().contains("maxDepth 1"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## EncodeOptions

`EncodeOptions::default()` preserves canonical TOON v3.3. Each extension below contrasts default-off output with opt-in output and round-trips through `parse_toon`. Fallbacks are lossless: when an opt-in extension is not eligible, the encoder emits the canonical shape instead of changing the value.

## `nested_tabular_headers`

Use recursive table headers for uniform nested object columns. Spec: [Nested tabular headers](../../docs/proposals/nested-tabular-headers.md).

```rust
use reddb_io_toon::{EncodeOptions, Value};

let value = Value::from_json_str(
    r#"{"orders":[{"id":1,"customer":{"name":"Ada","country":"UK"},"total":10.5},{"id":2,"customer":{"name":"Bob","country":"US"},"total":20}]}"#,
)?;

let off = value.to_canonical_toon();
assert_eq!(
    off,
    "orders[2]:\n  - id: 1\n    customer:\n      name: Ada\n      country: UK\n    total: 10.5\n  - id: 2\n    customer:\n      name: Bob\n      country: US\n    total: 20\n"
);

let on = value.to_toon_with_options(EncodeOptions {
    nested_tabular_headers: true,
    ..EncodeOptions::default()
});
assert_eq!(
    on,
    "orders[2]{id,customer{name,country},total}:\n  1,Ada,UK,10.5\n  2,Bob,US,20\n"
);
assert_eq!(Value::parse_toon(&on)?.to_json_value(), value.to_json_value());
# Ok::<(), Box<dyn std::error::Error>>(())
```

If nested object columns are not uniform, the encoder falls back to canonical nested objects.

## `keyed_map_collapse`

Use compact rows for object maps whose values are uniform objects. Spec: [Keyed-map collapse](../../docs/proposals/keyed-map-collapse.md).

```rust
use reddb_io_toon::{EncodeOptions, Value};

let value = Value::from_json_str(
    r#"{"people":{"joe":{"first":"Joe","last":"Schmoe"},"mary":{"first":"Mary","last":"Jane"}}}"#,
)?;

assert_eq!(
    value.to_canonical_toon(),
    "people:\n  joe:\n    first: Joe\n    last: Schmoe\n  mary:\n    first: Mary\n    last: Jane\n"
);

let on = value.to_toon_with_options(EncodeOptions {
    keyed_map_collapse: true,
    ..EncodeOptions::default()
});
assert_eq!(on, "people{first,last}:\n  joe: Joe,Schmoe\n  mary: Mary,Jane\n");
assert_eq!(Value::parse_toon(&on)?.to_json_value(), value.to_json_value());
# Ok::<(), Box<dyn std::error::Error>>(())
```

Maps with non-uniform value objects fall back to canonical nested objects.

## `primitive_array_columns`

Use primitive list columns inside otherwise tabular object arrays. Eligibility requires the column values to be arrays of primitive scalar values; `null`, objects, nested arrays, and mixed column shapes fall back losslessly. Spec: [Primitive-array columns](../../docs/proposals/primitive-array-columns.md).

```rust
use reddb_io_toon::{EncodeOptions, Value};

let value = Value::from_json_str(
    r#"{"items":[{"id":1,"tags":["hot","fragile"],"note":"a,b"},{"id":2,"tags":["semi;quoted"],"note":"plain"}]}"#,
)?;

assert_eq!(
    value.to_canonical_toon(),
    "items[2]:\n  - id: 1\n    tags[2]: hot,fragile\n    note: \"a,b\"\n  - id: 2\n    tags[1]: semi;quoted\n    note: plain\n"
);

let on = value.to_toon_with_options(EncodeOptions {
    primitive_array_columns: true,
    ..EncodeOptions::default()
});
assert_eq!(
    on,
    "items[2]{id,tags[;],note}:\n  1,hot;fragile,\"a,b\"\n  2,\"semi;quoted\",plain\n"
);
assert_eq!(Value::parse_toon(&on)?.to_json_value(), value.to_json_value());

let ineligible = Value::from_json_str(
    r#"{"items":[{"id":1,"tags":null},{"id":2,"tags":["ok"]}]}"#,
)?;
assert_eq!(
    ineligible.to_toon_with_options(EncodeOptions {
        primitive_array_columns: true,
        ..EncodeOptions::default()
    }),
    ineligible.to_canonical_toon()
);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## `object_array_columns`

Use child tables for array-valued object columns. Eligibility requires array-valued columns whose rows can be represented as child tables or fixed primitive matrices; mixed scalar/object columns fall back losslessly. Spec: [Child tables and matrix](../../docs/proposals/child-tables-and-matrix.md).

```rust
use reddb_io_toon::{EncodeOptions, Value};

let value = Value::from_json_str(
    r#"{"orders":[{"id":1,"items":[{"sku":"a","qty":2},{"sku":"b","qty":1}]},{"id":2,"items":[]}]}"#,
)?;

assert_eq!(
    value.to_canonical_toon(),
    "orders[2]:\n  - id: 1\n    items[2]{sku,qty}:\n      a,2\n      b,1\n  - id: 2\n    items: []\n"
);

let on = value.to_toon_with_options(EncodeOptions {
    object_array_columns: true,
    ..EncodeOptions::default()
});
assert_eq!(on, "orders[2]{id,items{sku,qty}}:\n  1,2\n    a,2\n    b,1\n  2,0\n");
assert_eq!(Value::parse_toon(&on)?.to_json_value(), value.to_json_value());

let ineligible = Value::from_json_str(
    r#"{"orders":[{"id":1,"items":[{"sku":"a"}]},{"id":2,"items":[1]}]}"#,
)?;
assert_eq!(
    ineligible.to_toon_with_options(EncodeOptions {
        object_array_columns: true,
        ..EncodeOptions::default()
    }),
    ineligible.to_canonical_toon()
);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## `cyclic_discriminated_arrays`

Use the specialized cyclic wire for eligible top-level event arrays. Eligibility requires a strongly repeated discriminator cycle, object rows, enough repeats to beat canonical output, and header-token-safe common fields. Boundary cases fall back losslessly. Spec: [Cyclic discriminated arrays](../../docs/proposals/cyclic-discriminated-arrays.md).

```rust
use reddb_io_toon::{EncodeOptions, Value};

let value = Value::from_json_str(
    r#"{"events":[{"type":"login","tenant":"acme","seq":1,"actor":"u1","ok":true},{"type":"purchase","tenant":"acme","seq":2,"actor":"u1","amount":12.5,"currency":"USD"},{"type":"logout","tenant":"acme","seq":3,"actor":"u1","durationMs":1200},{"type":"login","tenant":"acme","seq":4,"actor":"u2","ok":true},{"type":"purchase","tenant":"acme","seq":5,"actor":"u2","amount":4,"currency":"EUR"},{"type":"logout","tenant":"acme","seq":6,"actor":"u2","durationMs":900},{"type":"login","tenant":"acme","seq":7,"actor":"u3","ok":false},{"type":"purchase","tenant":"acme","seq":8,"actor":"u3","amount":99.95,"currency":"USD"},{"type":"logout","tenant":"acme","seq":9,"actor":"u3","durationMs":1800},{"type":"login","tenant":"acme","seq":10,"actor":"u4","ok":true},{"type":"purchase","tenant":"acme","seq":11,"actor":"u4","amount":1.25,"currency":"BRL"},{"type":"logout","tenant":"acme","seq":12,"actor":"u4","durationMs":600}]}"#,
)?;

let off = value.to_canonical_toon();
let on = value.to_toon_with_options(EncodeOptions {
    cyclic_discriminated_arrays: true,
    ..EncodeOptions::default()
});

assert_ne!(on, off);
assert!(on.starts_with("events:\n  order: cycle(login,purchase,logout)*4\n"));
assert!(on.contains("  common[12|]{tenant|seq|actor}:\n"));
assert!(on.contains("  purchase[4|]{amount|currency}:\n"));
assert_eq!(Value::parse_toon(&on)?.to_json_value(), value.to_json_value());

let ineligible = Value::from_json_str(
    r#"{"events":[{"type":"login","seq":1},{"type":"purchase","seq":2},{"type":"login","seq":3},{"type":"purchase","seq":4}]}"#,
)?;
assert_eq!(
    ineligible.to_toon_with_options(EncodeOptions {
        cyclic_discriminated_arrays: true,
        ..EncodeOptions::default()
    }),
    ineligible.to_canonical_toon()
);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The expected opt-in output begins:

```text
events:
  order: cycle(login,purchase,logout)*4
  discriminator: type
  rows: 12
  common[12|]{tenant|seq|actor}:
    acme|1|u1
    acme|2|u1
    acme|3|u1
  login[4|]{ok}:
    true
```

## `delimiter`

Select comma, pipe, or tab for array and tabular rows. Spec: [Delimiter choice](../../docs/proposals/delimiter-choice.md).

```rust
use reddb_io_toon::{EncodeOptions, Value};

let value = Value::from_json_str(r#"{"rows":[{"id":1,"name":"Ada"}]}"#)?;

assert_eq!(value.to_canonical_toon(), "rows[1]{id,name}:\n  1,Ada\n");

let pipe = value.to_toon_with_options(EncodeOptions {
    delimiter: '|',
    ..EncodeOptions::default()
});
assert_eq!(pipe, "rows[1|]{id|name}:\n  1|Ada\n");
assert_eq!(Value::parse_toon(&pipe)?.to_json_value(), value.to_json_value());
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `try_to_toon_with_options` when the delimiter may come from user input; invalid delimiters return `EncodeError`.

## detect_truncation

`detect_truncation(input)` checks TOON with default parse options. `detect_truncation_with_options(input, options)` checks TOON with explicit parse options. The report model is specified in [detectTruncation](../../docs/proposals/detect-truncation.md).

```rust
use reddb_io_toon::detect_truncation;

let report = detect_truncation("items[2]:\n  - one\n");
assert!(!report.complete);
assert_eq!(report.to_json_value()["kind"], "array_length_mismatch");
assert_eq!(report.to_json_value()["declared"], 2);
assert_eq!(report.to_json_value()["actual"], 1);
```

## TOONL

TOONL v0.2 is the append-only stream profile: a header opens a segment, rows append one per line, and trailers close segments when available. v0.2 covers resumable cursors, header-preserving trim, tagged multiplexing, close transforms, and append-safe retry patterns.

```rust
use reddb_io_toon::{detect_toonl_truncation, encode_toonl_values, ToonlReader, Value};
use std::io::Cursor;

let rows = vec![
    Value::from_json_str(r#"{"id":1,"name":"Ada"}"#)?,
    Value::from_json_str(r#"{"id":2,"name":"Linus"}"#)?,
];

let stream = encode_toonl_values(&rows)?;
assert_eq!(stream, "[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n");

let decoded = ToonlReader::new(Cursor::new(stream.as_bytes()))
    .collect::<Result<Vec<_>, _>>()?;
assert_eq!(decoded, rows);

let truncated = detect_toonl_truncation("[]{id}:\n1\n");
assert!(!truncated.complete);
# Ok::<(), Box<dyn std::error::Error>>(())
```

- `ToonlEncoder::new(delimiter, fields)` writes one fixed-schema segment.
- `ToonlWriter::new(writer)` and `ToonlWriter::with_delimiter(writer, delimiter)` write multi-segment streams and tagged lanes.
- `encode_toonl_values(values)` is the buffered encoder for record values.
- `ToonlReader::new(reader)` iterates decoded row values from any `BufRead`.
- `ToonlReader::cursor()` and `ToonlReader::resume_from_bytes(input, cursor)` support resumable reads.
- `ToonlStream::parse(input)`, `row_values()`, `close_transform_documents()`, and `close_transform_interleaved_documents()` parse and close streams.
- `jsonl_to_toonl(reader, writer)`, `toonl_to_jsonl(reader, writer)`, `close_transform_stream(reader, writer)`, and `close_transform_stream_interleaved(reader, writer)` bridge streaming formats.
- `detect_toonl_truncation(input)` reports missing or mismatched TOONL trailers.

The `tq` binary exposes header-preserving trim as `tq trim --keep-last N`; see [`crates/tq/README.md`](../tq/README.md).

## License

[MIT](../../LICENSE).
