# reddb-io-tq

> **Attribution:** This is RedDB's CLI for TOON - not the original project. The TOON format was created by Johann Schopplich; see the [official repo](https://github.com/toon-format/toon), [toon-format/spec](https://github.com/toon-format/spec), and [toonformat.dev](https://toonformat.dev) for the format spec and original project.

`tq` is a jq-style query CLI and converter for JSON, YAML, TOON, and TOONL.

It is shipped by the `reddb-io-tq` crate and uses the `reddb-io-toon` library. The TOON extension behavior is specified in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md), and TOONL v0.2 is specified in [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md).

```bash
cargo install reddb-io-tq --version 0.8.0
```

## Usage

```text
tq [-p toon|json|toonl|yaml|yml] [-o toon|json|toonl] [-r] [-c] [-s|--slurp] [--delimiter comma|tab|pipe] [--nested-tabular-headers] [--keyed-map-collapse] [--primitive-array-columns] [--object-array-columns] [--cyclic-discriminated-arrays] <query> [file]
tq trim --keep-last N [--in-place] [FILE]
tq close [--per-lane|--interleaved] [FILE]
tq check [-p toon|toonl] [FILE]
```

Format matrix:

| Flag | Formats | Notes |
| --- | --- | --- |
| `-p` | `toon`, `json`, `toonl`, `yaml`, `yml` | Selects input. File input defaults from `.toon`, `.json`, `.toonl`, `.yaml`, or `.yml`. |
| `-o` | `toon`, `json`, `toonl` | Selects output. YAML is input-only. |

## Query

The default subcommand is the query pipeline. `.` keeps the current value; field, index, slice, and builtin filters are evaluated by the CLI test suite.

Input:

```json
{"users":[{"id":1,"name":"Ada"},{"id":2,"name":"Linus"}]}
```

Command:

```bash
tq -p json -o toon '.users[0]'
```

Output:

```toon
id: 1
name: Ada
```

YAML input works with either `-p yaml` or `-p yml`.

Input:

```yaml
users:
  - id: 1
    name: Ada
```

Command:

```bash
tq -p yaml -o json -c .
```

Output:

```json
{"users":[{"id":1,"name":"Ada"}]}
```

Useful query flags:

- `-r` prints raw scalar strings.
- `-c` prints compact JSON.
- `-s` or `--slurp` collects TOONL rows into one array before evaluating the query.

## TOON Output Extensions

TOON output is canonical v3.3 unless an extension flag is enabled. These flags map directly to `reddb_io_toon::EncodeOptions`.

## `--nested-tabular-headers`

Input:

```json
{"orders":[{"id":1,"customer":{"name":"Ada","country":"UK"},"total":10.5},{"id":2,"customer":{"name":"Bob","country":"US"},"total":20}]}
```

Command:

```bash
tq -p json -o toon --nested-tabular-headers .
```

Output:

```toon
orders[2]{id,customer{name,country},total}:
  1,Ada,UK,10.5
  2,Bob,US,20
```

Spec: [Nested tabular headers](../../docs/proposals/nested-tabular-headers.md).

## `--keyed-map-collapse`

Input:

```json
{"people":{"joe":{"first":"Joe","last":"Schmoe"},"mary":{"first":"Mary","last":"Jane"}}}
```

Command:

```bash
tq -p json -o toon --keyed-map-collapse .
```

Output:

```toon
people{first,last}:
  joe: Joe,Schmoe
  mary: Mary,Jane
```

Spec: [Keyed-map collapse](../../docs/proposals/keyed-map-collapse.md).

## `--primitive-array-columns`

Input:

```json
{"items":[{"id":1,"tags":["hot","fragile"],"note":"a,b"},{"id":2,"tags":["semi;quoted"],"note":"plain"}]}
```

Command:

```bash
tq -p json -o toon --primitive-array-columns .
```

Output:

```toon
items[2]{id,tags[;],note}:
  1,hot;fragile,"a,b"
  2,"semi;quoted",plain
```

Spec: [Primitive-array columns](../../docs/proposals/primitive-array-columns.md).

## `--object-array-columns`

Input:

```json
{"orders":[{"id":1,"items":[{"sku":"a","qty":2},{"sku":"b","qty":1}]},{"id":2,"items":[]}]}
```

Command:

```bash
tq -p json -o toon --object-array-columns .
```

Output:

```toon
orders[2]{id,items{sku,qty}}:
  1,2
    a,2
    b,1
  2,0
```

Spec: [Child tables and matrix](../../docs/proposals/child-tables-and-matrix.md).

## `--cyclic-discriminated-arrays`

Input:

```json
{"events":[{"type":"login","tenant":"acme","seq":1,"actor":"u1","ok":true},{"type":"purchase","tenant":"acme","seq":2,"actor":"u1","amount":12.5,"currency":"USD"},{"type":"logout","tenant":"acme","seq":3,"actor":"u1","durationMs":1200},{"type":"login","tenant":"acme","seq":4,"actor":"u2","ok":true},{"type":"purchase","tenant":"acme","seq":5,"actor":"u2","amount":4,"currency":"EUR"},{"type":"logout","tenant":"acme","seq":6,"actor":"u2","durationMs":900},{"type":"login","tenant":"acme","seq":7,"actor":"u3","ok":false},{"type":"purchase","tenant":"acme","seq":8,"actor":"u3","amount":99.95,"currency":"USD"},{"type":"logout","tenant":"acme","seq":9,"actor":"u3","durationMs":1800},{"type":"login","tenant":"acme","seq":10,"actor":"u4","ok":true},{"type":"purchase","tenant":"acme","seq":11,"actor":"u4","amount":1.25,"currency":"BRL"},{"type":"logout","tenant":"acme","seq":12,"actor":"u4","durationMs":600}]}
```

Command:

```bash
tq -p json -o toon --cyclic-discriminated-arrays .
```

Output:

```text
events:
  order: cycle(login,purchase,logout)*4
  discriminator: type
  rows: 12
  common[12|]{tenant|seq|actor}:
    acme|1|u1
    acme|2|u1
    acme|3|u1
    acme|4|u2
    acme|5|u2
    acme|6|u2
    acme|7|u3
    acme|8|u3
    acme|9|u3
    acme|10|u4
    acme|11|u4
    acme|12|u4
  login[4|]{ok}:
    true
    true
    false
    true
  purchase[4|]{amount|currency}:
    12.5|USD
    4|EUR
    99.95|USD
    1.25|BRL
  logout[4|]{durationMs}:
    1200
    900
    1800
    600
```

Spec: [Cyclic discriminated arrays](../../docs/proposals/cyclic-discriminated-arrays.md).

## `--delimiter`

Input:

```json
{"rows":[{"id":1,"name":"Ada"}]}
```

Command:

```bash
tq -p json -o toon --delimiter pipe .
```

Output:

```toon
rows[1|]{id|name}:
  1|Ada
```

Spec: [Delimiter choice](../../docs/proposals/delimiter-choice.md).

## TOONL Query

TOONL input reads one flat record per row. Without `--slurp`, the query runs once per row.

Input:

```toonl
[]{id,name}:
1,Ada
2,Linus
[=2]
```

Command:

```bash
tq -p toonl -o json -c .name
```

Output:

```json
"Ada"
"Linus"
```

TOONL output writes append-only segments and rotates schemas as needed.

Input:

```jsonl
{"id":1,"name":"Ada"}
{"id":2,"name":"Linus"}
```

Command:

```bash
tq -p json -o toonl .
```

Output:

```toonl
[]{id,name}:
1,Ada
2,Linus
[=2]
```

## close

`tq close` materializes TOONL into canonical closed TOON documents.

Input:

```toonl
[]<req>{method,path,status}:
[]<metric>{name,value}:
req:GET,/health,200
metric:cpu,0.42
[]{event}:
[~]{event}:
started
req:POST,/login,401
metric:mem,0.70
```

Command:

```bash
tq close
```

Output:

```toon
[2]{method,path,status}:
  GET,/health,200
  POST,/login,401
[2]{name,value}:
  cpu,0.42
  mem,0.70
[1]{event}:
  started
```

`tq close --interleaved` preserves tagged row-run interleaving.

## trim

`tq trim --keep-last N` applies the TOONL v0.2 header-preserving suffix trim.

Input:

```toonl
[]{id,name}:
1,Ada
2,Linus
3,Grace
[=3]
```

Command:

```bash
tq trim --keep-last 2
```

Output:

```toonl
[]{id,name}:
2,Linus
3,Grace
[=2]
```

`--in-place` writes the file atomically and requires an explicit file path.

## check

`tq check` runs structured truncation detection for TOON or TOONL and prints JSON.

Input:

```toon
items[2]:
  - one
```

Command:

```bash
tq check -p toon
```

Output:

```json
{
  "complete": false,
  "kind": "array_length_mismatch",
  "line": 1,
  "declared": 2,
  "actual": 1,
  "message": "array declared 2 rows but found 1"
}
```

Complete input exits successfully. Truncated or invalid input exits non-zero and reports `complete`, `kind`, `line`, `declared`, `actual`, and `message`. The report model is specified in [detectTruncation](../../docs/proposals/detect-truncation.md).

## License

[MIT](../../LICENSE).
