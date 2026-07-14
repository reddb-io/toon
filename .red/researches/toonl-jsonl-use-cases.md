# What the world puts in JSONL — and what that demands of a TOONL line

Research for issue [#32](https://github.com/reddb-io/tq/issues/32), feeding the TOONL v0.1 design question:
**does a TOONL line need to carry only flat tabular rows, or must it also support nested TOON "frames" per record?**

Method: primary-source survey (specs, official docs, tool READMEs) of the major JSONL/NDJSON producer and
consumer classes, then a synthesis of what each class demands of line content.

---

## 1. The line-format "specs" themselves

Two competing quasi-specs define the envelope. Neither says anything about what a record looks like inside —
both allow *any* JSON value per line, so nesting is legal everywhere by construction.

### JSON Lines ([jsonlines.org](https://jsonlines.org/))

- **UTF-8** required; no BOM.
- **One valid JSON value per line** — any value, not just objects (`null` alone is a valid line).
- **Blank lines are forbidden.**
- Separator is `\n`; `\r\n` tolerated because JSON ignores surrounding whitespace. Trailing newline
  "strongly recommended but not required".
- No comments (a comment is not a valid JSON value).
- Extension `.jsonl` recommended.

### NDJSON ([ndjson-spec](https://github.com/ndjson/ndjson-spec))

- Each line **MUST** be an RFC 8259 JSON text, written followed by `\n` (optionally preceded by `\r`).
- **UTF-8** mandatory.
- Parsers **MAY silently ignore empty lines** — "This behavior MUST be documented and SHOULD be
  configurable by the user." (The one place the two specs diverge: jsonlines forbids blanks, ndjson lets
  parsers tolerate them.)
- No comment mechanism.
- Unparsable line → parser SHOULD raise an error. MIME `application/x-ndjson`.

**Takeaway for TOONL:** the ecosystem's baseline contract is *line = one complete value, UTF-8, `\n`,
no comments, blank lines at best implementation-defined*. TOONL should keep exactly this envelope
(it is what makes `grep`/`tail -f`/`split` work) and decide the flat-vs-nested question on the *content* level,
where the JSON specs are silent.

---

## 2. Survey by use-case class

### 2.1 Structured logging

| Producer | Record shape | Nesting in practice |
|---|---|---|
| [pino](https://github.com/pinojs/pino/blob/main/docs/api.md) | `{"level":30,"time":1531254555820,"pid":55956,"hostname":"x","msg":...}` | **Flat by default**; merged properties land at the root. But the standard `err` serializer nests `{type,msg,stack}` under `err`, and `nestedKey` wraps all bindings under one key. Any error line is nested. |
| [zap](https://pkg.go.dev/go.uber.org/zap) (production config) | `{"level":"info","ts":...,"caller":...,"msg":...}` | Flat core, but `zap.Object`, `zap.Namespace`, `zap.Dict`, `zap.Any` produce arbitrary nesting and are idiomatic: `{"level":"info","msg":"new request","req":{"url":"/test","remote":{"ip":"127.0.0.1","port":31200}}}` |
| bunyan | `{"name","hostname","pid","level","msg","time","v"}` | Same pattern as pino (it's the ancestor): flat core fields + nested `err` (with `stack`), `req`, `res` serializers. |
| [`journalctl -o json`](https://man7.org/linux/man-pages/man1/journalctl.1.html) | One JSON object per line, journal fields as keys (`MESSAGE`, `PRIORITY`, `_PID`, …) | **Genuinely flat** — values are strings, with two escape hatches: repeated fields become a JSON array of strings, and binary/non-UTF-8 fields become arrays of byte numbers. Fields >4096 bytes become `null` unless `--all`. |
| [OTLP file exporter](https://opentelemetry.io/docs/specs/otel/protocol/file-exporter/) | Explicitly "a JSON lines file"; each line is one `TracesData` / `MetricsData` / `LogsData` | **Deeply nested by design** — resource → scope → records → attributes, 4+ levels. Also: "There is no guarantee that the data in the file is ordered." |

**Pattern:** log lines have a *flat, stable spine* (level/time/msg/service) plus *shallow, optional nested
appendages* (`err`, `req`, per-call context). Key/field sets **drift line-to-line constantly** — every log
statement contributes its own merged fields — and missing fields are the norm, not the exception. Field
order is stable per producer but nothing consumes it positionally.

### 2.2 ML fine-tuning datasets and batch APIs — the headline JSONL use-case, and it is nested

- **OpenAI supervised fine-tuning** ([docs](https://developers.openai.com/api/docs/guides/supervised-fine-tuning)):
  "Use JSONL format, with one complete JSON structure on every line", minimum 10 lines. Each record is a
  chat sample: top-level `messages[]` of `{role, content}`, optionally `tools[]` with full JSON-Schema
  function definitions and `tool_calls[]` nested inside assistant messages (3–4 levels deep). **100% of
  records are nested** — a messages array is the whole point.
- **OpenAI Batch API** ([docs](https://developers.openai.com/api/docs/guides/batch)): one request per line,
  `{"custom_id", "method", "url", "body": {model, messages[…], …}}` — envelope is flat-ish, `body` is an
  arbitrarily nested request.
- **Anthropic Message Batches** ([docs](https://platform.claude.com/docs/en/docs/build-with-claude/batch-processing)):
  results are delivered as `.jsonl` "where each line is a valid JSON object representing the result of a
  single request", keyed by `custom_id`; each result embeds a full Message object (content blocks array,
  usage, etc.). Nested, several levels.
- **Hugging Face `datasets`** ([loading docs](https://huggingface.co/docs/datasets/en/loading)): declares the
  one-object-per-line layout "the most efficient format"; its canonical example is flat
  (`{"a": 1, "b": 2.0, "c": "foo", "d": false}`), but nested fields are first-class (Arrow struct/list
  features), `null`s expected, and schema is inferred by Arrow across lines. Real Hub datasets (chat/SFT
  sets) are predominantly nested `messages`/`conversations` records.

**Pattern:** the ML class is bimodal. Classic single-turn datasets (`{"prompt": ..., "completion": ...}`,
classification rows) are flat and map perfectly to tabular rows. But everything chat-era —
fine-tuning, batch inference in/out, eval traces — is **irreducibly nested** (arrays of message objects,
tool schemas). No flat encoding captures it without embedding a document in a cell.

### 2.3 Event streams (Kafka)

- **kcat `-J`** ([README](https://github.com/edenhill/kcat), [Confluent usage docs](https://docs.confluent.io/platform/current/tools/kafkacat-usage.html)):
  one JSON envelope per line — `{"topic","partition","offset","tstype","ts","broker","headers"?,"key","payload"}`.
  The envelope itself is **flat**; the `payload` is either an embedded JSON-escaped string or a nested value.
- Kafka Connect JSON sink/source and most "topic dump" tooling follow the same shape: a small flat transport
  envelope wrapping an opaque, often nested, domain payload.

**Pattern:** flat envelope + document payload. A rows-only format can carry the envelope if the payload can
live in one string cell.

### 2.4 Analytics / data-tool import & export

- **BigQuery** ([load JSON docs](https://docs.cloud.google.com/bigquery/docs/loading-data-cloud-storage-json)):
  "JSON data must be newline-delimited… Each JSON object must be on a separate line." Nested and repeated
  fields are first-class (`RECORD`, `REPEATED`); schema autodetect samples the file; `ignore_unknown_values`
  drops extra fields; missing fields become `NULL`. Field order irrelevant.
- **ClickHouse `JSONEachRow`** ([format docs](https://clickhouse.com/docs/interfaces/formats/JSONEachRow),
  [format settings](https://clickhouse.com/docs/en/operations/settings/formats)): aliases literally include
  `JSONLines`/`NDJSON`/`JSONL`. Strict by default: unknown fields throw unless
  `input_format_skip_unknown_fields=1`; omitted fields need `input_format_defaults_for_omitted_fields`;
  nested objects need `input_format_import_nested_json=1`. i.e. the fast path is **flat rows against a fixed
  table schema**, with nesting and drift as opt-in tolerances.
- **DuckDB `read_json`** ([JSON docs](https://duckdb.org/docs/stable/data/json/overview)): `format=newline_delimited`
  is one of three modes; nested JSON becomes `STRUCT`/`LIST` columns; schema inference samples up to
  `maximum_sample_size` records and takes the **union of keys** — missing keys become `NULL`; type conflicts
  coerce or widen; `ignore_errors` skips malformed lines.

**Pattern:** analytics engines assume *mostly-uniform objects*, treat missing fields as `NULL` as a matter of
course, ignore field order entirely, and tolerate drift by union-of-keys or by explicit opt-in flags. They
support nesting, but their sweet spot — and the reason NDJSON import exists — is the flat row.

### 2.5 Crawl and bulk data dumps

- **Common Crawl index (CDXJ)** ([announcement](https://commoncrawl.org/blog/announcing-the-common-crawl-index)):
  each line is `urlkey timestamp {flat json dict}` — url, WARC filename/offset/length, digest, mime, status.
  **Flat, extensible dict**; new fields "may be added … as needed". (Note: not even pure JSONL — a line has a
  sortable text prefix *before* the JSON. The successor [columnar index](https://commoncrawl.org/blog/index-to-warc-files-and-urls-in-columnar-format)
  is flat Parquet columns — the same data went *tabular* when efficiency mattered.)
- Common Crawl WAT metadata files, by contrast, are deeply nested JSON documents per record.
- **Elasticsearch `_bulk`** ([API docs](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-bulk)):
  the most widely deployed NDJSON consumer is not even homogeneous — action/metadata lines **alternate** with
  source-document lines, two schemas interleaved in one stream; final line must end with `\n`; documents must
  not be pretty-printed. Line semantics can depend on the *previous* line.

---

## 3. Cross-cutting answers to the ticket's questions

**Flat vs nested — how often?**
Roughly three clusters:
1. **Flat-with-warts (~ tabular):** journald exports, CDX indexes, analytics imports/exports, Kafka envelopes,
   classic prompt/completion datasets, metrics. Flat spine; occasional shallow nesting.
2. **Flat spine + shallow optional appendage:** pino/zap/bunyan logs — flat until an `err`/`req`/context
   object appears, which in practice is *most interesting lines* (every error carries a nested stack).
3. **Irreducibly nested documents:** LLM fine-tune/batch JSONL (`messages[]`), OTLP lines, WAT records.
   No flattening exists; these are documents that happen to be newline-framed.

**Schema drift mid-file?** Ubiquitous in logging (per-statement fields), expected by every analytics reader
(DuckDB union-of-keys, BigQuery `ignore_unknown_values`, ClickHouse skip-unknown flags), and *structural* in
Elasticsearch bulk (alternating schemas). Only fixed-schema exports (journald, CDX) hold a stable field set.

**Blank lines / comments?** Comments: nowhere. Blank lines: forbidden by jsonlines, "MAY silently ignore" in
ndjson — i.e. writers never emit them, robust readers often tolerate them.

**Field order stable?** Producers emit stable order (serializer determinism), but **no surveyed consumer
assigns meaning to order** — JSON object semantics are name-based. This matters: TOONL tabular rows are
*positional*, which is a semantic tightening relative to every JSONL consumer surveyed.

**Missing fields / null?** Endemic and always tolerated by consumers (→ `NULL`/absent). A positional row
format therefore **must** have an explicit null/empty cell marker — absence-by-omission doesn't exist in a
positional row.

---

## 4. What this demands of a TOONL line

Three demands fall out of the survey that are *independent* of the flat-vs-nested cut, and one that decides it:

1. **The `[N]` count cannot survive.** TOON's tabular header `deploys[4]{id,version}:` requires knowing the
   row count up front. Every use-case above is append-only/streaming — a logger, a batch writer, `tail -f` —
   where N is unknowable at header-write time. A TOONL header must drop the count (or make it an optional
   trailing checksum), e.g. `deploys{id,version}:`. This is the single biggest divergence from TOON the
   research forces.
2. **Headers must be re-emittable mid-stream.** Schema drift is the norm (logs, union-of-keys readers).
   A TOONL stream needs "header restatement": a new header line switches the active schema for subsequent
   rows. This also gives free `cat a.toonl b.toonl` concatenability — a property JSONL has and users rely on.
3. **Explicit null cells.** Positional rows + endemic missing fields ⇒ the row syntax needs a first-class
   null/absent marker per cell (TOON already has `null`; TOONL rows will use it *much* more than TOON docs do).
4. **The nested question (the ticket's core):** see matrix.

### Recommendation matrix

| Use-case class | Example producers | Rows-only TOONL v0.1? | Needs nested frames? |
|---|---|---|---|
| Fixed-schema exports/indexes | journald `-o json`, CDX index, metrics | **Yes — ideal fit**, biggest token win vs JSONL (keys once) | No |
| Analytics import/export | BigQuery, ClickHouse JSONEachRow, DuckDB nd | **Yes** for flat tables (the common case); nested RECORD/STRUCT columns lose fidelity | Partially (nested columns) |
| Classic ML datasets | prompt/completion, classification rows | **Yes** | No |
| App logs (pino/zap/bunyan) | flat spine + `err`/`req` objects | **Degraded** — fine until the first error line | Yes, shallow (1–2 levels) |
| Kafka/event envelopes | kcat `-J`, Connect dumps | **Yes if** payload may be an escaped-string cell | Only for exploded payloads |
| LLM fine-tune / batch JSONL | OpenAI SFT & Batch, Anthropic Batches | **No** — `messages[]` is irreducible | Yes, deep |
| Telemetry documents | OTLP file exporter, CC WAT | **No** | Yes, deep |

### Recommendation for the v0.1 cut

**Ship v0.1 rows-only — with the three streaming demands above (countless header, header restatement,
explicit nulls) and one escape hatch: a cell may contain a JSON-escaped string.** Defer nested frames to
v0.2, but *reserve the syntax now* (e.g. a line-start sigil that cannot begin a row or header) so v0.1 files
are forward-compatible.

Rationale, with honest trade-offs:

- **Where TOONL wins is exactly where rows suffice.** The token/size advantage over JSONL comes from
  stating keys once and rows positionally — that mechanism only pays off on repeated-shape records, i.e.
  clusters 1–2. For nested documents (cluster 3) TOON's own spec already refuses tabular form ("All values
  across these keys are primitives (no nested arrays/objects)", [TOON SPEC](https://github.com/toon-format/spec/blob/main/SPEC.md))
  and falls back to indented list items — which are *multi-line* and therefore fight the one-record-per-line
  framing. Nested frames in TOONL means designing a single-line folded encoding of a TOON subtree: a real
  spec effort with its own quoting/delimiter questions. Rushing it into v0.1 risks baking in a bad fold.
- **The honest cost:** rows-only v0.1 cannot natively represent the highest-visibility JSONL use-case of the
  moment (LLM `messages[]` files) or the nested-`err` half of logging. The escaped-string cell keeps those
  streams *transportable* (envelope in TOONL columns, document in one cell — exactly the kcat pattern) but at
  zero token savings for the nested part, and with double-escaping ugliness. That is acceptable for v0.1
  precisely because for those payloads TOONL has no advantage to offer yet anyway.
- **What v0.1 must not do:** claim the `[N]` count, forbid header restatement, or leave null cells implicit.
  Any of those would make TOONL unusable for the streaming/appending consumers that are the entire reason a
  line-oriented variant exists.

---

## Sources

- [JSON Lines specification](https://jsonlines.org/)
- [NDJSON spec](https://github.com/ndjson/ndjson-spec)
- [TOON format spec](https://github.com/toon-format/spec/blob/main/SPEC.md)
- [pino API docs](https://github.com/pinojs/pino/blob/main/docs/api.md)
- [zap package docs](https://pkg.go.dev/go.uber.org/zap)
- [journalctl(1) man page](https://man7.org/linux/man-pages/man1/journalctl.1.html)
- [OTLP File Exporter spec](https://opentelemetry.io/docs/specs/otel/protocol/file-exporter/)
- [OpenAI supervised fine-tuning guide](https://developers.openai.com/api/docs/guides/supervised-fine-tuning)
- [OpenAI Batch API guide](https://developers.openai.com/api/docs/guides/batch)
- [Anthropic Message Batches docs](https://platform.claude.com/docs/en/docs/build-with-claude/batch-processing)
- [Hugging Face datasets loading guide](https://huggingface.co/docs/datasets/en/loading)
- [kcat README](https://github.com/edenhill/kcat) / [Confluent kcat usage](https://docs.confluent.io/platform/current/tools/kafkacat-usage.html)
- [BigQuery: loading JSON from Cloud Storage](https://docs.cloud.google.com/bigquery/docs/loading-data-cloud-storage-json)
- [ClickHouse JSONEachRow format](https://clickhouse.com/docs/interfaces/formats/JSONEachRow) / [format settings](https://clickhouse.com/docs/en/operations/settings/formats)
- [DuckDB JSON loading](https://duckdb.org/docs/stable/data/json/overview)
- [Common Crawl index announcement (CDXJ)](https://commoncrawl.org/blog/announcing-the-common-crawl-index) / [columnar index](https://commoncrawl.org/blog/index-to-warc-files-and-urls-in-columnar-format)
- [Elasticsearch Bulk API](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-bulk)
