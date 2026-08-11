# reddb-io-tq

> **Attribution:** This is RedDB's CLI for TOON - not the original project. The TOON format was created by Johann Schopplich; see the [official repo](https://github.com/toon-format/toon), [toon-format/spec](https://github.com/toon-format/spec), and [toonformat.dev](https://toonformat.dev) for the format spec and original project.

`tq` is a jq-style query CLI and converter for JSON, YAML, XML, TOON, and TOONL.

It is shipped by the `reddb-io-tq` crate and uses the `reddb-io-toon` library. The TOON extension behavior is specified in [`docs/toon-reddb-spec.md`](../../docs/toon-reddb-spec.md), and TOONL v0.2 is specified in [`docs/toonl-reddb-spec.md`](../../docs/toonl-reddb-spec.md).

```bash
cargo install reddb-io-tq --version 0.20.0
```

## Usage

```text
tq [-p toon|json|toonl|yaml|yml|xml] [-o toon|json|toonl|xml] [-r] [-c] [-j] [-S] [-e] [-s|--slurp] [--strict|--no-strict] [--delimiter comma|tab|pipe] [--primitive-array-columns] [--object-array-columns] [--cyclic-discriminated-arrays] <query> [file]
tq trim --keep-last N [--in-place] [FILE]
tq close [--per-lane|--interleaved] [FILE]
tq check [-p toon|toonl] [FILE]
```

Format matrix:

| Flag | Formats | Notes |
| --- | --- | --- |
| `-p` | `toon`, `json`, `toonl`, `yaml`, `yml`, `xml` | Selects input. File input defaults from `.toon`, `.json`, `.toonl`, `.yaml`, `.yml`, or `.xml`; an XML-shaped stdin document is also detected. |
| `-o` | `toon`, `json`, `toonl`, `xml` | Selects output. YAML is input-only. |

## Query

The default subcommand is the query pipeline. `.` keeps the current value;
field, index, slice, and builtin filters are evaluated by the CLI test suite.
The supported language surface, including the UTC-only time builtins, is
documented in the [tq language reference](../../docs/tq-language.md).

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

## XML conversion

XML input uses one explicit tree shape, so element names are never guessed as
object keys and repeated elements are never guessed to be arrays. As with YAML,
XML input defaults to TOON output. Use `-p xml` for stdin, or pass an `.xml`
file; `tq` also recognizes stdin beginning with unambiguous XML markup.

```bash
printf '%s' '<items xmlns:x="urn:item"><x:item id="1"/>tail</items>' \
  | tq -p xml .
```

```toon
xml:
  declaration: null
  children[1]:
    - type: element
      name: items
      attributes[1]{name,value}:
        "xmlns:x","urn:item"
      children[2]:
        - type: element
          name: "x:item"
          attributes[1]{name,value}:
            id,"1"
          children: []
          empty: true
        - type: text
          value: tail
      empty: false
```

The canonical value is `{xml: {declaration, children}}`. A declaration records
`version` and optional `encoding` and `standalone` fields. Every child is an
ordered node with a `type`: `element`, `text`, `cdata`, `comment`, or
`processing_instruction`. Elements retain their qualified `name`, ordered
`attributes` as `{name, value}` records (including `xmlns` declarations),
ordered `children`, and an `empty` flag that distinguishes `<x/>` from
`<x></x>`. Processing instructions use `target` and `value` fields.

`tq -o xml` accepts only this canonical tree. This deliberate requirement
prevents an element-vs-array heuristic when converting JSON or TOON. DTDs are
rejected, parsing is depth- and node-bounded, and malformed input returns a
bounded diagnostic with a non-zero status.

Useful query flags:

- `-r` prints raw scalar strings.
- `-c` prints compact JSON.
- `-j` implies raw string output and omits the newline after each result.
- `-S` sorts object keys recursively before encoding the selected output format.
- `-e` returns status 0 when the last result is truthy, 1 when it is `false` or
  `null`, and 4 when the query produces no result.
- `-s` or `--slurp` collects TOONL rows into one array before evaluating the query.

## TOON v4.1 output and extensions

TOON input is strict v4.1 by default; `--no-strict` is an explicit legacy
recovery mode. Output is canonical v4.1 unless one of the three local wire
extension flags below is enabled. Nested field groups and keyed tabular form
are already canonical v4.1 and are selected automatically for eligible values.
For script compatibility, the CLI still accepts `--nested-tabular-headers` and
`--keyed-map-collapse` as deprecated no-op switches; neither changes output.

For example, canonical conversion needs no feature flag:

```bash
printf '%s\n' '{"people":{"joe":{"first":"Joe","last":"Schmoe"},"mary":{"first":"Mary","last":"Jane"}}}' \
  | tq -p json -o toon .
```

```toon
people[2:]{first,last}:
  joe: Joe,Schmoe
  mary: Mary,Jane
```

The remaining flags map to opt-in fields on
`reddb_io_toon::EncodeV4Options`; their wire formats are userland-only and
fall back losslessly to canonical v4.1 when a value is ineligible.

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
