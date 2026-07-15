# Benchmark Dataset Manifest

The token benchmark reads this vendored corpus offline. The shape taxonomy is
deliberately anti-cherry-pick: every class is represented independently of
whether TOON, TOONL, JSON, JSONL, CSV, YAML, or XML wins. `wide-sparse` is
included because it is a known weak shape for TOON-style repeated-key layouts.

Each representative shape has `small` and `large` variants. Small variants keep
the original few-record smoke-sized snapshots. Large variants use realistic LLM
payload sizes: a page of repository results, an API route catalog, an audit
event batch, a knowledge tree, an activity feed, an append-only log segment, or
a sparse feature batch.

| Dataset | Shape class | Variant | Record count | Provenance | Source and license | Exercises |
| --- | --- | --- | ---: | --- | --- | --- |
| `flat-tabular/public-repositories-small.json` | flat-tabular | small | 6 | real-vendored factual snapshot | Public GitHub repository metadata; factual fields from public repository pages/APIs, license data from each repository license field. Facts are not copyrightable; repository code licenses include MIT, Apache-2.0, GPL-2.0-only, Python-2.0. | Uniform scalar records where CSV/TOONL can compete honestly. |
| `flat-tabular/public-repositories-large.json` | flat-tabular | large | 48 | extended real-vendored factual snapshot | Additional public repository names, owners, primary languages, default branches, and license identifiers from public repository metadata. Facts are not copyrightable; licenses remain attributed by SPDX-like identifier. | One page of public repository search or discovery results. |
| `nested-uniform/openapi-petstore-paths-small.json` | nested-uniform | small | 3 | real-vendored adapted snapshot | Swagger/OpenAPI Petstore example structure, Apache-2.0. | Repeated nested endpoint records with uniform response arrays. |
| `nested-uniform/openapi-petstore-paths-large.json` | nested-uniform | large | 96 | deterministic schema-derived expansion | Derived from the Petstore path record shape with local deterministic resource names and repeated request/response schema references; no external fetch is needed. | A realistic service route catalog with repeated nested request and response objects. |
| `nested-heterogeneous/json-schema-event-small.json` | nested-heterogeneous | small | 2 | real-vendored adapted snapshot | JSON Schema 2020-12 vocabulary examples and audit-event domain facts, JSON Schema docs are MIT licensed. | Mixed schema objects, `oneOf`, arrays, open-ended scalar values. |
| `nested-heterogeneous/json-schema-event-large.json` | nested-heterogeneous | large | 80 | deterministic schema-derived expansion | Reuses the small snapshot's JSON Schema and generates deterministic audit examples from that schema shape. | A batch of heterogeneous audit events with user/service actors and optional diffs. |
| `deep-tree/wikidata-knowledge-tree-small.json` | deep-tree | small | 7 | real-vendored factual snapshot | Wikidata entity identifiers and labels, CC0 public-domain dedication. | Recursive object depth and repeated nested statement/value pairs. |
| `deep-tree/wikidata-knowledge-tree-large.json` | deep-tree | large | 109 | extended real-vendored factual snapshot | Additional Wikidata entity identifiers and English labels, CC0 public-domain dedication, arranged into a deterministic local tree. | A deeper knowledge graph excerpt with repeated branch statements. |
| `tagged-records/activity-events-small.json` | tagged-records | small | 4 | schema-generated deterministic | Local deterministic event schema, no external source. | Discriminated records with type-specific payload fields. |
| `tagged-records/activity-events-large.json` | tagged-records | large | 120 | deterministic schema-derived expansion | Generated from the small tagged-record event schema with deterministic IDs, actors, timestamps, and payload fields. | A realistic issue/activity feed page across several event types. |
| `streaming-append/append-only-logs-small.json` | streaming-append | small | 6 | schema-generated deterministic | Local deterministic append-log schema, no external source. | JSONL/TOONL stream shape with append-only record ordering. |
| `streaming-append/append-only-logs-large.json` | streaming-append | large | 160 | deterministic schema-derived expansion | Generated from the append-log schema with monotonic sequence numbers, timestamps, services, levels, messages, request IDs, and latencies. | A log segment or event batch suitable for streaming encoders. |
| `wide-sparse/sparse-feature-vectors-small.json` | wide-sparse | small | 5 | schema-generated deterministic | Local deterministic sparse-feature schema, no external source. | Wide sparse objects with mostly unique keys where repeated-key formats can lose. |
| `wide-sparse/sparse-feature-vectors-large.json` | wide-sparse | large | 96 | deterministic schema-derived expansion | Generated from the sparse-feature schema with stable sparse key families and mostly row-specific columns. | A sparse search or feature-vector batch that preserves TOON's weak case. |

## Anti-Cherry-Pick Register

- The corpus includes both real-vendored snapshots and deterministic generated
  fixtures.
- Every representative shape has `small` and `large` variants, and the
  benchmark report measures both sizes before summarizing by shape.
- The shape classes were chosen before measuring this report.
- `wide-sparse` remains in the representative corpus even when it produces
  worse TOON/TOONL results than minified JSON, JSONL, CSV, YAML, or XML.
- Wire corpora from `tests/corpus/wire-efficiency/` are still measured, but the
  report labels them as extension-eligibility showcase fixtures, not
  representative payload evidence.
