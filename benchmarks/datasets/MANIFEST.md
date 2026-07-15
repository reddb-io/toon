# Benchmark Dataset Manifest

The token benchmark reads this vendored corpus offline. The shape taxonomy is
deliberately anti-cherry-pick: every class is represented independently of
whether TOON, TOONL, JSON, JSONL, CSV, YAML, or XML wins. `wide-sparse` is
included because it is a known weak shape for TOON-style repeated-key layouts.

| Dataset | Shape class | Provenance | Source and license | Size | Exercises |
| --- | --- | --- | --- | ---: | --- |
| `flat-tabular/public-repositories.json` | flat-tabular | real-vendored factual snapshot | Public GitHub repository metadata; factual fields from public repository pages/APIs, license data from each repository license field. Facts are not copyrightable; repository code licenses include MIT, Apache-2.0, GPL-2.0-only, Python-2.0. | 6 rows | Uniform scalar records where CSV/TOONL can compete honestly. |
| `nested-uniform/openapi-petstore-paths.json` | nested-uniform | real-vendored adapted snapshot | Swagger/OpenAPI Petstore example structure, Apache-2.0. | 3 endpoints | Repeated nested endpoint records with uniform response arrays. |
| `nested-heterogeneous/json-schema-event.json` | nested-heterogeneous | real-vendored adapted snapshot | JSON Schema 2020-12 vocabulary examples and audit-event domain facts, JSON Schema docs are MIT licensed. | 1 schema, 2 examples | Mixed schema objects, `oneOf`, arrays, open-ended scalar values. |
| `deep-tree/wikidata-knowledge-tree.json` | deep-tree | real-vendored factual snapshot | Wikidata entity identifiers and labels, CC0 public-domain dedication. | 1 entity tree | Recursive object depth and repeated nested statement/value pairs. |
| `tagged-records/activity-events.json` | tagged-records | schema-generated deterministic | Local deterministic event schema, no external source. | 4 events | Discriminated records with type-specific payload fields. |
| `streaming-append/append-only-logs.json` | streaming-append | schema-generated deterministic | Local deterministic append-log schema, no external source. | 6 log records | JSONL/TOONL stream shape with append-only record ordering. |
| `wide-sparse/sparse-feature-vectors.json` | wide-sparse | schema-generated deterministic | Local deterministic sparse-feature schema, no external source. | 5 feature rows | Wide sparse objects with mostly unique keys where repeated-key formats can lose. |

## Anti-Cherry-Pick Register

- The corpus includes both real-vendored snapshots and deterministic generated
  fixtures.
- The shape classes were chosen before measuring this report.
- `wide-sparse` remains in the representative corpus even when it produces
  worse TOON/TOONL results than minified JSON, JSONL, CSV, YAML, or XML.
- Wire corpora from `tests/corpus/wire-efficiency/` are still measured, but the
  report labels them as extension-eligibility showcase fixtures, not
  representative payload evidence.
