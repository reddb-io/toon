# reddb-io-tq

> **Attribution:** This is RedDB's CLI for TOON - not the original project. The TOON format was created by Johann Schopplich; see the [official repo](https://github.com/toon-format/toon), [toon-format/spec](https://github.com/toon-format/spec), and [toonformat.dev](https://toonformat.dev) for the format spec and original project.

`tq` is a jq-style query CLI and converter for JSON, YAML, TOON, and TOONL.

It is shipped by the `reddb-io-tq` crate and uses the `reddb-io-toon` library. The TOON extension behavior is specified in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md), TOONL v0.2 is specified in [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md), and performance notes live in [`benchmarks/`](../../benchmarks/README.md).

```bash
cargo install reddb-io-tq
```

## Usage

```text
tq [-p toon|json|toonl|yaml|yml] [-o toon|json|toonl] [-r] [-c] [-s|--slurp] [--delimiter comma|tab|pipe] [--nested-tabular-headers] [--keyed-map-collapse] [--primitive-array-columns] [--object-array-columns] [--cyclic-discriminated-arrays] <query> [file]
tq trim --keep-last N [--in-place] [FILE]
tq close [--per-lane|--interleaved] [FILE]
tq check [-p toon|toonl] [FILE]
```

The input format defaults from the file extension when a file is provided. Use `-p` for stdin or when the extension is ambiguous. YAML input is accepted with either `-p yaml` or `-p yml`; output formats are `toon`, `json`, and `toonl`.

## Query And Convert

```bash
printf '{"users":[{"id":1,"name":"Ada"}]}' | tq -p json -o toon .
```

```toon
users[1]{id,name}:
  1,Ada
```

```bash
printf 'users:\n  - id: 1\n    name: Ada\n' | tq -p yaml -o json -c .
```

```json
{"users":[{"id":1,"name":"Ada"}]}
```

- `<query>` is the field/index/slice pipeline used by the test suite. `.` keeps the current value.
- `-p toon|json|toonl|yaml|yml` selects input.
- `-o toon|json|toonl` selects output.
- `-r` prints raw scalar strings.
- `-c` prints compact JSON.
- `-s` or `--slurp` collects TOONL rows into one array before evaluating the query.

## TOON Output Extensions

TOON output is canonical v3.3 unless an extension flag is enabled. These flags map directly to `reddb_io_toon::EncodeOptions`.

- `--nested-tabular-headers` emits recursive table headers for uniform nested object columns. Spec: [Nested tabular headers](../../docs/proposals/nested-tabular-headers.md).
- `--keyed-map-collapse` emits compact rows for object maps whose values are uniform objects. Spec: [Keyed-map collapse](../../docs/proposals/keyed-map-collapse.md).
- `--primitive-array-columns` emits primitive list columns such as `tags[;]` inside otherwise tabular object arrays. Spec: [Primitive-array columns](../../docs/proposals/primitive-array-columns.md).
- `--object-array-columns` emits child tables for array-valued object columns. Spec: [Child tables and matrix](../../docs/proposals/child-tables-and-matrix.md).
- `--cyclic-discriminated-arrays` emits the specialized wire for eligible top-level event arrays whose discriminator values repeat in a stable cycle. Spec: [Cyclic discriminated arrays](../../docs/proposals/cyclic-discriminated-arrays.md).
- `--delimiter comma|tab|pipe` selects the active array and tabular delimiter. Spec: [Delimiter choice](../../docs/proposals/delimiter-choice.md).

```bash
printf '{"rows":[{"id":1,"tags":["red","blue"]}]}' \
  | tq -p json -o toon --primitive-array-columns .
```

```toon
rows[1]{id,tags[;]}:
  1,red;blue
```

## TOONL

TOONL input reads one flat record per row. TOONL output writes append-only segments and rotates schemas as needed.

```bash
printf '{"id":1,"name":"Ada"}\n{"id":2,"name":"Linus"}\n' | tq -p json -o toonl .
```

```toonl
[]{id,name}:
1,Ada
2,Linus
[=2]
```

- `-p toonl` streams rows through the query. Without `--slurp`, the query runs once per row.
- `--slurp` evaluates the query against a single array of all TOONL rows.
- `-o toonl` emits TOONL using the same encoder flags where applicable.
- Tagged multiplexing is part of the TOONL v0.2 library surface; the CLI reads tagged streams and the `close` command can render them per-lane or interleaved.

## close

`tq close` materializes TOONL into canonical closed TOON documents.

```bash
tq close events.toonl
tq close --interleaved events.toonl
```

- `--per-lane` is the default. Each lane segment becomes one canonical TOON document.
- `--interleaved` preserves tagged row-run interleaving for post-mortem rendering.

## trim

`tq trim --keep-last N` applies the TOONL v0.2 header-preserving suffix trim.

```bash
tq trim --keep-last 100 events.toonl
tq trim --keep-last 100 --in-place events.toonl
```

The trimmed output keeps the headers needed to make the retained suffix readable. `--in-place` writes the file atomically and requires an explicit file path.

## check

`tq check` runs structured truncation detection for TOON or TOONL and prints JSON.

```bash
tq check -p toon document.toon
tq check -p toonl stream.toonl
```

Complete input exits successfully. Truncated or invalid input exits non-zero and reports `complete`, `kind`, `line`, `declared`, `actual`, and `message`. The report model is specified in [detectTruncation](../../docs/proposals/detect-truncation.md).

## License

[MIT](../../LICENSE).
