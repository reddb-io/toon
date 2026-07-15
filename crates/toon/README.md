# reddb-io-toon

> **Attribution:** This is RedDB's Rust implementation of TOON - not the original project. The TOON format was created by Johann Schopplich; see the [official repo](https://github.com/toon-format/toon), [toon-format/spec](https://github.com/toon-format/spec), and [toonformat.dev](https://toonformat.dev) for the format spec and original project.

Rust parser, serializer, document model, and TOONL v0.2 stream utilities for TOON v3.3.

The crate implements canonical TOON v3.3 by default, the reddb-io opt-in encode extensions in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md), and the TOONL streaming layer in [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md). Performance notes live in [`benchmarks/`](../../benchmarks/README.md), not in this package README.

```toml
[dependencies]
reddb-io-toon = "0.8"
```

## TOON

```rust
use reddb_io_toon::Value;

let value = Value::parse_toon("users[1]{id,name}:\n  1,Ada\n")?;
assert_eq!(value.to_canonical_toon(), "users[1]{id,name}:\n  1,Ada\n");
# Ok::<(), Box<dyn std::error::Error>>(())
```

- `Value::parse_toon(input)` decodes any TOON root value.
- `Value::parse_with_options(input, ParseOptions { .. })` adds decoder options: `indent`, `strict`, `expand_paths`, and `max_depth`.
- `Document::parse(input)` and `Document::parse_with_options(input, options)` require an object root.
- `Value::from_json_str(input)` and `Value::from_json_value(value)` convert JSON into the crate's value model.
- `Value::to_canonical_toon()` and `Document::to_canonical_toon()` emit canonical TOON.
- `try_to_canonical_toon()` and `try_to_toon_with_options(options)` return `EncodeError` instead of panicking on impossible encoding options such as an invalid delimiter.
- `to_json_value()` and `to_json_string(compact)` convert back to `serde_json`.
- `ParseError::line()` and `ParseError::message()` expose source diagnostics. `EncodeError::message()` exposes encode diagnostics.

Strict mode is on by default through `ParseOptions::default()`. Set `strict: false` only when accepting legacy recovery behavior is intentional.

## EncodeOptions

`EncodeOptions::default()` preserves canonical TOON v3.3. Every extension below is decode always-on and encode opt-in.

```rust
use reddb_io_toon::{EncodeOptions, Value};

let value = Value::from_json_str(r#"{"rows":[{"id":1,"name":"Ada"}]}"#)?;
let toon = value.to_toon_with_options(EncodeOptions {
    delimiter: '|',
    ..EncodeOptions::default()
});
assert_eq!(toon, "rows[1|]{id|name}:\n  1|Ada\n");
# Ok::<(), Box<dyn std::error::Error>>(())
```

- `nested_tabular_headers` emits recursive table headers for uniform nested object columns. Spec: [Nested tabular headers](../../docs/proposals/nested-tabular-headers.md).
- `keyed_map_collapse` emits compact rows for object maps whose values are uniform objects. Spec: [Keyed-map collapse](../../docs/proposals/keyed-map-collapse.md).
- `primitive_array_columns` emits primitive list columns such as `tags[;]` inside otherwise tabular object arrays. Spec: [Primitive-array columns](../../docs/proposals/primitive-array-columns.md).
- `object_array_columns` emits child tables for array-valued object columns. Spec: [Child tables and matrix](../../docs/proposals/child-tables-and-matrix.md).
- `cyclic_discriminated_arrays` emits the specialized wire for eligible top-level event arrays whose discriminator values repeat in a stable cycle. Spec: [Cyclic discriminated arrays](../../docs/proposals/cyclic-discriminated-arrays.md).
- `delimiter` selects comma, pipe, or tab for array and tabular rows. Spec: [Delimiter choice](../../docs/proposals/delimiter-choice.md).
- `max_depth` bounds fallible encoding; `0` disables the guard for trusted input.

## detect_truncation

```rust
use reddb_io_toon::{detect_toonl_truncation, detect_truncation};

let toon_report = detect_truncation("items[2]:\n  - one\n");
assert!(!toon_report.complete);

let toonl_report = detect_toonl_truncation("[]{id}:\n1\n");
assert!(!toonl_report.complete);
```

- `detect_truncation(input)` checks TOON with default parse options.
- `detect_truncation_with_options(input, options)` checks TOON with explicit parse options.
- `detect_toonl_truncation(input)` checks TOONL trailers.
- `TruncationReport::to_json_value()` returns the structured report used by the CLI `check` command.

The report kinds are specified in [detectTruncation](../../docs/proposals/detect-truncation.md).

## TOONL

TOONL v0.2 is the append-only stream profile: a header opens a segment, rows append one per line, and trailers close segments when available. v0.2 covers resumable cursors, header-preserving trim, tagged multiplexing, close transforms, and append-safe retry patterns.

```rust
use reddb_io_toon::{encode_toonl_values, ToonlReader, Value};
use std::io::Cursor;

let rows = vec![Value::from_json_str(r#"{"id":1,"name":"Ada"}"#)?];
let stream = encode_toonl_values(&rows)?;
let decoded = ToonlReader::new(Cursor::new(stream.into_bytes()))
    .collect::<Result<Vec<_>, _>>()?;
assert_eq!(decoded.len(), 1);
# Ok::<(), Box<dyn std::error::Error>>(())
```

- `ToonlEncoder::new(delimiter, fields)` writes one fixed-schema segment. Use `push_raw_row`, `push_value_row`, continuation setters, and `finish()`.
- `ToonlWriter::new(writer)` and `ToonlWriter::with_delimiter(writer, delimiter)` write multi-segment record streams, rotate on schema changes, and support `declare_lane` plus `write_tagged_record` for tagged multiplexing.
- `encode_toonl_values(values)` is the buffered encoder for record values.
- `ToonlReader::new(reader)` iterates decoded row values from any `BufRead`.
- `ToonlReader::cursor()` returns a `ToonlCursor` for resumable reads.
- `ToonlReader::resume_from_bytes(input, cursor)` resumes from a prior cursor and returns `ToonlResumeError` when truncation or anchor mismatch invalidates it.
- `ToonlCursor::to_json_string()` and `ToonlCursor::from_json_str(input)` serialize cursor state.
- `ToonlStream::parse(input)` parses a complete stream into segments.
- `ToonlStream::row_values()` decodes all rows.
- `ToonlStream::close_transform_documents()` returns one canonical TOON document per lane segment.
- `ToonlStream::close_transform_interleaved_documents()` preserves tagged row-run interleaving.
- `jsonl_to_toonl(reader, writer)` and `toonl_to_jsonl(reader, writer)` bridge JSONL and TOONL.
- `close_transform_stream(reader, writer)` and `close_transform_stream_interleaved(reader, writer)` stream the close-transform output.
- `ToonlError`, `ToonlCursorInvalidation`, and `ToonlResumeError` report TOONL failures.

The `tq` binary exposes header-preserving trim as `tq trim --keep-last N`; see [`crates/tq/README.md`](../tq/README.md).

## License

[MIT](../../LICENSE).
