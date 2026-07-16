# Token Efficiency Benchmark

Command: `pnpm benchmark:tokens`

Tokenizer: `o200k_base` via `gpt-tokenizer`.

Representative datasets are vendored under `benchmarks/datasets/` and are read offline. Canonical TOON is measured through both `@reddb-io/toon` and the Rust crate via the shipped `tq` CLI; extension formats are measured through the JS package implementation. Wire fixtures are retained as an extension-eligibility showcase, not as representative payload evidence.

## Representative Corpus by Shape

| Shape | Datasets | Best TOON-family median vs JSON | Best non-TOON median vs JSON |
| --- | ---: | ---: | ---: |
| cyclic-discriminated-arrays | 4 | toon-ext-all (-26.8%) | csv (-26.3%) |
| deep-tree | 2 | toon-ext-all (-35.1%) | yaml (37.9%) |
| flat-tabular | 2 | toonl (-48.1%) | csv (-47.7%) |
| nested-heterogeneous | 2 | toon-ext-all (12.5%) | yaml (44.2%) |
| nested-uniform | 2 | toon-ext-all (-32.3%) | yaml (40.5%) |
| streaming-append | 2 | toonl (-32.8%) | csv (-33.9%) |
| tagged-records | 2 | toon-ext-all (19.5%) | yaml (38.8%) |
| wide-sparse | 2 | toonl (-7.4%) | jsonl (0.8%) |

## Amortization Curve by Shape and Size

The crossover point is the first measured record count where the best TOON-family format uses no more tokens than minified JSON. `not observed` means both measured sizes still lose to JSON for that shape.

| Shape | Variant | Record count | JSON minified tokens | Best TOON-family format | Best TOON-family tokens | Tokens vs JSON | Crossover record count |
| --- | --- | ---: | ---: | --- | ---: | ---: | ---: |
| cyclic-discriminated-arrays | 24-minimal | 24 | 905 | toon-ext-cyclic-discriminated-arrays | 716 | -20.9% | 24 |
| cyclic-discriminated-arrays | 90-rich | 90 | 3905 | toon-ext-cyclic-discriminated-arrays | 2862 | -26.7% | 24 |
| cyclic-discriminated-arrays | 240-rich | 240 | 10500 | toon-ext-cyclic-discriminated-arrays | 7640 | -27.2% | 24 |
| cyclic-discriminated-arrays | 500-rich | 500 | 21435 | toon-ext-cyclic-discriminated-arrays | 15676 | -26.9% | 24 |
| deep-tree | small | 7 | 163 | toon-ext-all | 126 | -22.7% | 7 |
| deep-tree | large | 109 | 2505 | toon-ext-all | 1313 | -47.6% | 7 |
| flat-tabular | small | 6 | 250 | toonl | 141 | -43.6% | 6 |
| flat-tabular | large | 48 | 1941 | toonl | 919 | -52.7% | 6 |
| nested-heterogeneous | small | 2 | 447 | toon-v3.3-canonical | 509 | 13.9% | not observed |
| nested-heterogeneous | large | 80 | 8459 | toon-v3.3-canonical | 9405 | 11.2% | not observed |
| nested-uniform | small | 3 | 268 | toon-ext-child-tables | 199 | -25.7% | 3 |
| nested-uniform | large | 96 | 8345 | toon-ext-child-tables | 5106 | -38.8% | 3 |
| streaming-append | small | 6 | 286 | toonl | 201 | -29.7% | 6 |
| streaming-append | large | 160 | 7542 | toonl | 4829 | -36.0% | 6 |
| tagged-records | small | 4 | 216 | toon-v3.3-canonical | 258 | 19.4% | not observed |
| tagged-records | large | 120 | 6386 | toon-v3.3-canonical | 7632 | 19.5% | not observed |
| wide-sparse | small | 5 | 286 | toonl | 262 | -8.4% | 5 |
| wide-sparse | large | 96 | 5468 | toonl | 5118 | -6.4% | 5 |

## Explicit TOON/TOONL Losses

| Shape | Dataset | Format | Tokens vs minified JSON |
| --- | --- | --- | ---: |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | toon-v3.3-canonical | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | toon-rust-crate-canonical | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | toon-ext-primitive-array-columns | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | toon-ext-child-tables | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | toon-ext-delimiter-pipe | 18.1% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | toon-ext-keyed-map-collapse | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | toon-ext-cyclic-discriminated-arrays | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | toon-v3.3-canonical | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | toon-rust-crate-canonical | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | toon-ext-primitive-array-columns | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | toon-ext-child-tables | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | toon-ext-delimiter-pipe | 15.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | toon-ext-keyed-map-collapse | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | toon-ext-cyclic-discriminated-arrays | 12.3% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | toon-v3.3-canonical | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | toon-rust-crate-canonical | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | toon-ext-primitive-array-columns | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | toon-ext-child-tables | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | toon-ext-delimiter-pipe | 13.0% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | toon-ext-keyed-map-collapse | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | toon-ext-cyclic-discriminated-arrays | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | toon-ext-all | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | toon-v3.3-canonical | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | toon-rust-crate-canonical | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | toon-ext-primitive-array-columns | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | toon-ext-child-tables | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | toon-ext-delimiter-pipe | 16.8% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | toon-ext-keyed-map-collapse | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | toon-ext-cyclic-discriminated-arrays | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | toon-ext-all | 13.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | toon-v3.3-canonical | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | toon-rust-crate-canonical | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | toon-ext-primitive-array-columns | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | toon-ext-delimiter-pipe | 12.2% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | toon-ext-keyed-map-collapse | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | toon-ext-cyclic-discriminated-arrays | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | toon-v3.3-canonical | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | toon-rust-crate-canonical | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | toon-ext-primitive-array-columns | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | toon-ext-delimiter-pipe | 12.7% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | toon-ext-keyed-map-collapse | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | toon-ext-cyclic-discriminated-arrays | 10.1% |
| tagged-records | tagged-records/activity-events-large | toon-v3.3-canonical | 19.5% |
| tagged-records | tagged-records/activity-events-large | toon-rust-crate-canonical | 19.5% |
| tagged-records | tagged-records/activity-events-large | toon-ext-primitive-array-columns | 19.5% |
| tagged-records | tagged-records/activity-events-large | toon-ext-child-tables | 19.5% |
| tagged-records | tagged-records/activity-events-large | toon-ext-delimiter-pipe | 20.6% |
| tagged-records | tagged-records/activity-events-large | toon-ext-keyed-map-collapse | 19.5% |
| tagged-records | tagged-records/activity-events-large | toon-ext-cyclic-discriminated-arrays | 19.5% |
| tagged-records | tagged-records/activity-events-large | toon-ext-all | 19.5% |
| tagged-records | tagged-records/activity-events-small | toon-v3.3-canonical | 19.4% |
| tagged-records | tagged-records/activity-events-small | toon-rust-crate-canonical | 19.4% |
| tagged-records | tagged-records/activity-events-small | toon-ext-primitive-array-columns | 19.4% |
| tagged-records | tagged-records/activity-events-small | toon-ext-child-tables | 19.4% |
| tagged-records | tagged-records/activity-events-small | toon-ext-delimiter-pipe | 21.3% |
| tagged-records | tagged-records/activity-events-small | toon-ext-keyed-map-collapse | 19.4% |
| tagged-records | tagged-records/activity-events-small | toon-ext-cyclic-discriminated-arrays | 19.4% |
| tagged-records | tagged-records/activity-events-small | toon-ext-all | 19.4% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | toon-v3.3-canonical | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | toon-rust-crate-canonical | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | toon-ext-primitive-array-columns | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | toon-ext-child-tables | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | toon-ext-delimiter-pipe | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | toon-ext-keyed-map-collapse | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | toon-ext-cyclic-discriminated-arrays | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | toon-ext-all | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | toon-v3.3-canonical | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | toon-rust-crate-canonical | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | toon-ext-primitive-array-columns | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | toon-ext-child-tables | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | toon-ext-delimiter-pipe | 19.6% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | toon-ext-keyed-map-collapse | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | toon-ext-cyclic-discriminated-arrays | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | toon-ext-all | 19.2% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | toon-v3.3-canonical | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | toon-ext-primitive-array-columns | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | toon-ext-child-tables | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | toon-ext-delimiter-pipe | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | toon-ext-keyed-map-collapse | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | toonl | 12.7% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | toon-v3.3-canonical | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | toon-ext-primitive-array-columns | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | toon-ext-child-tables | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | toon-ext-delimiter-pipe | 20.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | toon-ext-keyed-map-collapse | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | toonl | 11.4% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | toon-v3.3-canonical | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | toon-ext-primitive-array-columns | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | toon-ext-child-tables | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | toon-ext-delimiter-pipe | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | toon-ext-keyed-map-collapse | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | toonl | 11.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | toon-v3.3-canonical | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | toon-ext-primitive-array-columns | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | toon-ext-child-tables | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | toon-ext-delimiter-pipe | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | toon-ext-keyed-map-collapse | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | toonl | 11.4% |

## Representative Dataset Measurements

| Shape | Dataset | Variant | Record count | Format | Bytes | Tokens | Tokens vs minified JSON |
| --- | --- | --- | ---: | --- | ---: | ---: | ---: |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | json-minified | 9182 | 2505 | 0.0% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | json-pretty | 40756 | 4658 | 85.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | yaml | 28799 | 3513 | 40.2% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | xml | 13264 | 3900 | 55.7% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | toon-v3.3-canonical | 24879 | 2904 | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | toon-rust-crate-canonical | 24879 | 2904 | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | toon-ext-primitive-array-columns | 24879 | 2904 | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | toon-ext-child-tables | 24879 | 2904 | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | toon-ext-delimiter-pipe | 24933 | 2958 | 18.1% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | toon-ext-keyed-map-collapse | 24879 | 2904 | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | toon-ext-cyclic-discriminated-arrays | 24879 | 2904 | 15.9% |
| deep-tree | deep-tree/wikidata-knowledge-tree-large | large | 109 | toon-ext-all | 4804 | 1313 | -47.6% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | json-minified | 557 | 163 | 0.0% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | json-pretty | 1460 | 297 | 82.2% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | yaml | 1008 | 221 | 35.6% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | xml | 816 | 249 | 52.8% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | toon-v3.3-canonical | 893 | 183 | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | toon-rust-crate-canonical | 893 | 183 | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | toon-ext-primitive-array-columns | 893 | 183 | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | toon-ext-child-tables | 893 | 183 | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | toon-ext-delimiter-pipe | 898 | 188 | 15.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | toon-ext-keyed-map-collapse | 893 | 183 | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | toon-ext-cyclic-discriminated-arrays | 893 | 183 | 12.3% |
| deep-tree | deep-tree/wikidata-knowledge-tree-small | small | 7 | toon-ext-all | 409 | 126 | -22.7% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | json-minified | 7835 | 1941 | 0.0% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | json-pretty | 11395 | 3193 | 64.5% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | yaml | 9128 | 2707 | 39.5% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | xml | 11507 | 2934 | 51.2% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toon-v3.3-canonical | 2873 | 963 | -50.4% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toon-rust-crate-canonical | 2873 | 963 | -50.4% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toon-ext-primitive-array-columns | 2873 | 963 | -50.4% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toon-ext-child-tables | 2873 | 963 | -50.4% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toon-ext-delimiter-pipe | 2874 | 1094 | -43.6% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toon-ext-keyed-map-collapse | 2873 | 963 | -50.4% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toon-ext-cyclic-discriminated-arrays | 2873 | 963 | -50.4% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toon-ext-all | 2873 | 963 | -50.4% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | jsonl | 7817 | 1984 | 2.2% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | csv | 2758 | 959 | -50.6% |
| flat-tabular | flat-tabular/public-repositories-large | large | 48 | toonl | 2769 | 919 | -52.7% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | json-minified | 1009 | 250 | 0.0% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | json-pretty | 1461 | 410 | 64.0% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | yaml | 1168 | 344 | 37.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | xml | 1489 | 379 | 51.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toon-v3.3-canonical | 456 | 146 | -41.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toon-rust-crate-canonical | 456 | 146 | -41.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toon-ext-primitive-array-columns | 456 | 146 | -41.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toon-ext-child-tables | 456 | 146 | -41.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toon-ext-delimiter-pipe | 457 | 162 | -35.2% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toon-ext-keyed-map-collapse | 456 | 146 | -41.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toon-ext-cyclic-discriminated-arrays | 456 | 146 | -41.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toon-ext-all | 456 | 146 | -41.6% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | jsonl | 991 | 251 | 0.4% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | csv | 426 | 138 | -44.8% |
| flat-tabular | flat-tabular/public-repositories-small | small | 6 | toonl | 436 | 141 | -43.6% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | json-minified | 28378 | 8459 | 0.0% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | json-pretty | 50931 | 14349 | 69.6% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | yaml | 37969 | 11884 | 40.5% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | xml | 38887 | 12506 | 47.8% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | toon-v3.3-canonical | 29963 | 9405 | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | toon-rust-crate-canonical | 29963 | 9405 | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | toon-ext-primitive-array-columns | 29963 | 9405 | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | toon-ext-child-tables | 29963 | 9405 | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | toon-ext-delimiter-pipe | 30057 | 9556 | 13.0% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | toon-ext-keyed-map-collapse | 29963 | 9405 | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | toon-ext-cyclic-discriminated-arrays | 29963 | 9405 | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-large | large | 80 | toon-ext-all | 29963 | 9405 | 11.2% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | json-minified | 1621 | 447 | 0.0% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | json-pretty | 3404 | 827 | 85.0% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | yaml | 2417 | 661 | 47.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | xml | 2374 | 711 | 59.1% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | toon-v3.3-canonical | 1966 | 509 | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | toon-rust-crate-canonical | 1966 | 509 | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | toon-ext-primitive-array-columns | 1966 | 509 | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | toon-ext-child-tables | 1966 | 509 | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | toon-ext-delimiter-pipe | 1975 | 522 | 16.8% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | toon-ext-keyed-map-collapse | 1966 | 509 | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | toon-ext-cyclic-discriminated-arrays | 1966 | 509 | 13.9% |
| nested-heterogeneous | nested-heterogeneous/json-schema-event-small | small | 2 | toon-ext-all | 1966 | 509 | 13.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | json-minified | 38013 | 8345 | 0.0% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | json-pretty | 59717 | 13982 | 67.5% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | yaml | 47322 | 11768 | 41.0% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | xml | 53294 | 12638 | 51.4% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | toon-v3.3-canonical | 35965 | 9174 | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | toon-rust-crate-canonical | 35965 | 9174 | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | toon-ext-primitive-array-columns | 35965 | 9174 | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | toon-ext-child-tables | 19858 | 5106 | -38.8% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | toon-ext-delimiter-pipe | 36158 | 9367 | 12.2% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | toon-ext-keyed-map-collapse | 35965 | 9174 | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | toon-ext-cyclic-discriminated-arrays | 35965 | 9174 | 9.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-large | large | 96 | toon-ext-all | 19858 | 5106 | -38.8% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | json-minified | 1185 | 268 | 0.0% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | json-pretty | 1871 | 450 | 67.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | yaml | 1473 | 375 | 39.9% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | xml | 1679 | 408 | 52.2% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | toon-v3.3-canonical | 1118 | 295 | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | toon-rust-crate-canonical | 1118 | 295 | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | toon-ext-primitive-array-columns | 1118 | 295 | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | toon-ext-child-tables | 728 | 199 | -25.7% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | toon-ext-delimiter-pipe | 1125 | 302 | 12.7% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | toon-ext-keyed-map-collapse | 1118 | 295 | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | toon-ext-cyclic-discriminated-arrays | 1118 | 295 | 10.1% |
| nested-uniform | nested-uniform/openapi-petstore-paths-small | small | 3 | toon-ext-all | 728 | 199 | -25.7% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | json-minified | 22874 | 7542 | 0.0% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | json-pretty | 33442 | 11546 | 53.1% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | yaml | 26711 | 10100 | 33.9% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | xml | 31213 | 10906 | 44.6% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toon-v3.3-canonical | 11247 | 5148 | -31.7% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toon-rust-crate-canonical | 11247 | 5148 | -31.7% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toon-ext-primitive-array-columns | 11247 | 5148 | -31.7% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toon-ext-child-tables | 11247 | 5148 | -31.7% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toon-ext-delimiter-pipe | 11248 | 5160 | -31.6% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toon-ext-keyed-map-collapse | 11247 | 5148 | -31.7% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toon-ext-cyclic-discriminated-arrays | 11247 | 5148 | -31.7% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toon-ext-all | 11247 | 5148 | -31.7% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | jsonl | 22861 | 7697 | 2.1% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | csv | 10592 | 4823 | -36.1% |
| streaming-append | streaming-append/append-only-logs-large | large | 160 | toonl | 10924 | 4829 | -36.0% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | json-minified | 852 | 286 | 0.0% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | json-pretty | 1256 | 440 | 53.8% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | yaml | 993 | 380 | 32.9% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | xml | 1183 | 416 | 45.5% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toon-v3.3-canonical | 465 | 212 | -25.9% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toon-rust-crate-canonical | 465 | 212 | -25.9% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toon-ext-primitive-array-columns | 465 | 212 | -25.9% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toon-ext-child-tables | 465 | 212 | -25.9% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toon-ext-delimiter-pipe | 466 | 214 | -25.2% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toon-ext-keyed-map-collapse | 465 | 212 | -25.9% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toon-ext-cyclic-discriminated-arrays | 465 | 212 | -25.9% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toon-ext-all | 465 | 212 | -25.9% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | jsonl | 839 | 287 | 0.3% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | csv | 428 | 195 | -31.8% |
| streaming-append | streaming-append/append-only-logs-small | small | 6 | toonl | 450 | 201 | -29.7% |
| tagged-records | tagged-records/activity-events-large | large | 120 | json-minified | 20360 | 6386 | 0.0% |
| tagged-records | tagged-records/activity-events-large | large | 120 | json-pretty | 33308 | 10261 | 60.7% |
| tagged-records | tagged-records/activity-events-large | large | 120 | yaml | 25767 | 8915 | 39.6% |
| tagged-records | tagged-records/activity-events-large | large | 120 | xml | 28808 | 9471 | 48.3% |
| tagged-records | tagged-records/activity-events-large | large | 120 | toon-v3.3-canonical | 22191 | 7632 | 19.5% |
| tagged-records | tagged-records/activity-events-large | large | 120 | toon-rust-crate-canonical | 22191 | 7632 | 19.5% |
| tagged-records | tagged-records/activity-events-large | large | 120 | toon-ext-primitive-array-columns | 22191 | 7632 | 19.5% |
| tagged-records | tagged-records/activity-events-large | large | 120 | toon-ext-child-tables | 22191 | 7632 | 19.5% |
| tagged-records | tagged-records/activity-events-large | large | 120 | toon-ext-delimiter-pipe | 22262 | 7703 | 20.6% |
| tagged-records | tagged-records/activity-events-large | large | 120 | toon-ext-keyed-map-collapse | 22191 | 7632 | 19.5% |
| tagged-records | tagged-records/activity-events-large | large | 120 | toon-ext-cyclic-discriminated-arrays | 22191 | 7632 | 19.5% |
| tagged-records | tagged-records/activity-events-large | large | 120 | toon-ext-all | 22191 | 7632 | 19.5% |
| tagged-records | tagged-records/activity-events-small | small | 4 | json-minified | 697 | 216 | 0.0% |
| tagged-records | tagged-records/activity-events-small | small | 4 | json-pretty | 1135 | 350 | 62.0% |
| tagged-records | tagged-records/activity-events-small | small | 4 | yaml | 869 | 298 | 38.0% |
| tagged-records | tagged-records/activity-events-small | small | 4 | xml | 990 | 321 | 48.6% |
| tagged-records | tagged-records/activity-events-small | small | 4 | toon-v3.3-canonical | 759 | 258 | 19.4% |
| tagged-records | tagged-records/activity-events-small | small | 4 | toon-rust-crate-canonical | 759 | 258 | 19.4% |
| tagged-records | tagged-records/activity-events-small | small | 4 | toon-ext-primitive-array-columns | 759 | 258 | 19.4% |
| tagged-records | tagged-records/activity-events-small | small | 4 | toon-ext-child-tables | 759 | 258 | 19.4% |
| tagged-records | tagged-records/activity-events-small | small | 4 | toon-ext-delimiter-pipe | 763 | 262 | 21.3% |
| tagged-records | tagged-records/activity-events-small | small | 4 | toon-ext-keyed-map-collapse | 759 | 258 | 19.4% |
| tagged-records | tagged-records/activity-events-small | small | 4 | toon-ext-cyclic-discriminated-arrays | 759 | 258 | 19.4% |
| tagged-records | tagged-records/activity-events-small | small | 4 | toon-ext-all | 759 | 258 | 19.4% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | json-minified | 16964 | 5468 | 0.0% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | json-pretty | 24076 | 8040 | 47.0% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | yaml | 19553 | 7170 | 31.1% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | xml | 27518 | 9089 | 66.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toon-v3.3-canonical | 18186 | 6533 | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toon-rust-crate-canonical | 18186 | 6533 | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toon-ext-primitive-array-columns | 18186 | 6533 | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toon-ext-child-tables | 18186 | 6533 | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toon-ext-delimiter-pipe | 18187 | 6534 | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toon-ext-keyed-map-collapse | 18186 | 6533 | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toon-ext-cyclic-discriminated-arrays | 18186 | 6533 | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toon-ext-all | 18186 | 6533 | 19.5% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | jsonl | 16950 | 5559 | 1.7% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | csv | 62022 | 16327 | 198.6% |
| wide-sparse | wide-sparse/sparse-feature-vectors-large | large | 96 | toonl | 15292 | 5118 | -6.4% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | json-minified | 877 | 286 | 0.0% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | json-pretty | 1255 | 423 | 47.9% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | yaml | 1009 | 372 | 30.1% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | xml | 1436 | 476 | 66.4% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toon-v3.3-canonical | 942 | 341 | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toon-rust-crate-canonical | 942 | 341 | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toon-ext-primitive-array-columns | 942 | 341 | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toon-ext-child-tables | 942 | 341 | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toon-ext-delimiter-pipe | 943 | 342 | 19.6% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toon-ext-keyed-map-collapse | 942 | 341 | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toon-ext-cyclic-discriminated-arrays | 942 | 341 | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toon-ext-all | 942 | 341 | 19.2% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | jsonl | 863 | 286 | 0.0% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | csv | 803 | 246 | -14.0% |
| wide-sparse | wide-sparse/sparse-feature-vectors-small | small | 5 | toonl | 779 | 262 | -8.4% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | json-minified | 2387 | 905 | 0.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | json-pretty | 3595 | 1365 | 50.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | yaml | 2816 | 1215 | 34.3% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | xml | 3101 | 1245 | 37.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toon-v3.3-canonical | 2531 | 1084 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toon-ext-primitive-array-columns | 2531 | 1084 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toon-ext-child-tables | 2531 | 1084 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toon-ext-delimiter-pipe | 2532 | 1085 | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toon-ext-keyed-map-collapse | 2531 | 1084 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toon-ext-cyclic-discriminated-arrays | 1465 | 716 | -20.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toon-ext-all | 1465 | 716 | -20.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toon-rust-ext-cyclic-discriminated-arrays | 1465 | 716 | -20.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | jsonl | 2375 | 924 | 2.1% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | csv | 1314 | 684 | -24.4% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle2-24-minimal | 24-minimal | 24 | toonl | 2159 | 1020 | 12.7% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | json-minified | 10361 | 3905 | 0.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | json-pretty | 15589 | 5889 | 50.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | yaml | 12248 | 5253 | 34.5% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | xml | 13439 | 5379 | 37.7% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toon-v3.3-canonical | 11021 | 4684 | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toon-ext-primitive-array-columns | 11021 | 4684 | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toon-ext-child-tables | 11021 | 4684 | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toon-ext-delimiter-pipe | 11022 | 4685 | 20.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toon-ext-keyed-map-collapse | 11021 | 4684 | 19.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toon-ext-cyclic-discriminated-arrays | 5632 | 2862 | -26.7% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toon-ext-all | 5632 | 2862 | -26.7% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toon-rust-ext-cyclic-discriminated-arrays | 5632 | 2862 | -26.7% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | jsonl | 10349 | 3990 | 2.2% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | csv | 5526 | 2809 | -28.1% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle3-90-rich | 90-rich | 90 | toonl | 9209 | 4350 | 11.4% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | json-minified | 27385 | 10500 | 0.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | json-pretty | 41313 | 15784 | 50.3% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | yaml | 32422 | 14098 | 34.3% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | xml | 34963 | 14396 | 37.1% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toon-v3.3-canonical | 29074 | 12577 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toon-ext-primitive-array-columns | 29074 | 12577 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toon-ext-child-tables | 29074 | 12577 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toon-ext-delimiter-pipe | 29075 | 12578 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toon-ext-keyed-map-collapse | 29074 | 12577 | 19.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toon-ext-cyclic-discriminated-arrays | 14897 | 7640 | -27.2% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toon-ext-all | 14897 | 7640 | -27.2% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toon-rust-ext-cyclic-discriminated-arrays | 14897 | 7640 | -27.2% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | jsonl | 27373 | 10735 | 2.2% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | csv | 15328 | 7678 | -26.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle4-240-rich | 240-rich | 240 | toonl | 24261 | 11659 | 11.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | json-minified | 55386 | 21435 | 0.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | json-pretty | 83594 | 32139 | 49.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | yaml | 65583 | 28733 | 34.0% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | xml | 70704 | 29330 | 36.8% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toon-v3.3-canonical | 58895 | 25645 | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toon-ext-primitive-array-columns | 58895 | 25645 | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toon-ext-child-tables | 58895 | 25645 | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toon-ext-delimiter-pipe | 58896 | 25646 | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toon-ext-keyed-map-collapse | 58895 | 25645 | 19.6% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toon-ext-cyclic-discriminated-arrays | 30354 | 15676 | -26.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toon-ext-all | 30354 | 15676 | -26.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toon-rust-ext-cyclic-discriminated-arrays | 30354 | 15676 | -26.9% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | jsonl | 55374 | 21930 | 2.3% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | csv | 32056 | 15922 | -25.7% |
| cyclic-discriminated-arrays | cyclic-discriminated-arrays/cycle5-500-rich | 500-rich | 500 | toonl | 49382 | 23872 | 11.4% |

## Wire Extension-Eligibility Showcase

These `wire-*` fixtures exercise opt-in extension behavior and edge cases. They are not representative corpus evidence.

| Dataset | Format | Bytes | Tokens | Tokens vs minified JSON |
| --- | --- | ---: | ---: | ---: |
| wire-shipments-500 | json-minified | 54164 | 16412 | 0.0% |
| wire-shipments-500 | json-pretty | 83172 | 26916 | 64.0% |
| wire-shipments-500 | yaml | 64661 | 23410 | 42.6% |
| wire-shipments-500 | xml | 79685 | 23995 | 46.2% |
| wire-shipments-500 | toon-v3.3-canonical | 20214 | 8758 | -46.6% |
| wire-shipments-500 | toon-ext-primitive-array-columns | 20214 | 8758 | -46.6% |
| wire-shipments-500 | toon-ext-child-tables | 20214 | 8758 | -46.6% |
| wire-shipments-500 | toon-ext-delimiter-pipe | 20215 | 9926 | -39.5% |
| wire-shipments-500 | toon-ext-keyed-map-collapse | 20214 | 8758 | -46.6% |
| wire-shipments-500 | toon-ext-cyclic-discriminated-arrays | 20214 | 8758 | -46.6% |
| wire-shipments-500 | toon-ext-all | 20214 | 8758 | -46.6% |
| wire-shipments-500 | jsonl | 54149 | 16906 | 3.0% |
| wire-shipments-500 | csv | 19197 | 9004 | -45.1% |
| wire-shipments-500 | toonl | 19209 | 8758 | -46.6% |
| wire-accounts-300 | json-minified | 33841 | 10651 | 0.0% |
| wire-accounts-300 | json-pretty | 62649 | 19655 | 84.5% |
| wire-accounts-300 | yaml | 43738 | 15449 | 45.0% |
| wire-accounts-300 | xml | 47361 | 16355 | 53.6% |
| wire-accounts-300 | toon-v3.3-canonical | 39242 | 13050 | 22.5% |
| wire-accounts-300 | toon-ext-primitive-array-columns | 39242 | 13050 | 22.5% |
| wire-accounts-300 | toon-ext-child-tables | 12289 | 5266 | -50.6% |
| wire-accounts-300 | toon-ext-delimiter-pipe | 39243 | 13051 | 22.5% |
| wire-accounts-300 | toon-ext-keyed-map-collapse | 12289 | 5266 | -50.6% |
| wire-accounts-300 | toon-ext-cyclic-discriminated-arrays | 39242 | 13050 | 22.5% |
| wire-accounts-300 | toon-ext-all | 12289 | 5266 | -50.6% |
| wire-registry-200 | json-minified | 18775 | 6004 | 0.0% |
| wire-registry-200 | json-pretty | 27383 | 9209 | 53.4% |
| wire-registry-200 | yaml | 21172 | 7603 | 26.6% |
| wire-registry-200 | xml | 25795 | 8609 | 43.4% |
| wire-registry-200 | toon-v3.3-canonical | 20571 | 7402 | 23.3% |
| wire-registry-200 | toon-ext-primitive-array-columns | 20571 | 7402 | 23.3% |
| wire-registry-200 | toon-ext-child-tables | 20571 | 7402 | 23.3% |
| wire-registry-200 | toon-ext-delimiter-pipe | 20571 | 7402 | 23.3% |
| wire-registry-200 | toon-ext-keyed-map-collapse | 10403 | 4412 | -26.5% |
| wire-registry-200 | toon-ext-cyclic-discriminated-arrays | 20571 | 7402 | 23.3% |
| wire-registry-200 | toon-ext-all | 10403 | 4412 | -26.5% |
| wire-services-250 | json-minified | 33542 | 9619 | 0.0% |
| wire-services-250 | json-pretty | 57550 | 17373 | 80.6% |
| wire-services-250 | yaml | 41789 | 14117 | 46.8% |
| wire-services-250 | xml | 48062 | 14873 | 54.6% |
| wire-services-250 | toon-v3.3-canonical | 39043 | 12618 | 31.2% |
| wire-services-250 | toon-ext-primitive-array-columns | 39043 | 12618 | 31.2% |
| wire-services-250 | toon-ext-child-tables | 14349 | 5137 | -46.6% |
| wire-services-250 | toon-ext-delimiter-pipe | 39044 | 12619 | 31.2% |
| wire-services-250 | toon-ext-keyed-map-collapse | 14349 | 5137 | -46.6% |
| wire-services-250 | toon-ext-cyclic-discriminated-arrays | 39043 | 12618 | 31.2% |
| wire-services-250 | toon-ext-all | 14349 | 5137 | -46.6% |
| wire-tagged-300 | json-minified | 24794 | 8113 | 0.0% |
| wire-tagged-300 | json-pretty | 46414 | 14753 | 81.8% |
| wire-tagged-300 | yaml | 35135 | 13415 | 65.4% |
| wire-tagged-300 | xml | 40591 | 14489 | 78.6% |
| wire-tagged-300 | toon-v3.3-canonical | 25359 | 10181 | 25.5% |
| wire-tagged-300 | toon-ext-primitive-array-columns | 12784 | 5723 | -29.5% |
| wire-tagged-300 | toon-ext-child-tables | 25359 | 10181 | 25.5% |
| wire-tagged-300 | toon-ext-delimiter-pipe | 25660 | 10482 | 29.2% |
| wire-tagged-300 | toon-ext-keyed-map-collapse | 25359 | 10181 | 25.5% |
| wire-tagged-300 | toon-ext-cyclic-discriminated-arrays | 25359 | 10181 | 25.5% |
| wire-tagged-300 | toon-ext-all | 12784 | 5723 | -29.5% |
| wire-matrix-150x8 | json-minified | 7616 | 4803 | 0.0% |
| wire-matrix-150x8 | json-pretty | 17524 | 7807 | 62.5% |
| wire-matrix-150x8 | yaml | 15263 | 8851 | 84.3% |
| wire-matrix-150x8 | xml | 23684 | 10207 | 112.5% |
| wire-matrix-150x8 | toon-v3.3-canonical | 8667 | 5702 | 18.7% |
| wire-matrix-150x8 | toon-ext-primitive-array-columns | 8667 | 5702 | 18.7% |
| wire-matrix-150x8 | toon-ext-child-tables | 7628 | 5107 | 6.3% |
| wire-matrix-150x8 | toon-ext-delimiter-pipe | 8818 | 5853 | 21.9% |
| wire-matrix-150x8 | toon-ext-keyed-map-collapse | 8667 | 5702 | 18.7% |
| wire-matrix-150x8 | toon-ext-cyclic-discriminated-arrays | 8667 | 5702 | 18.7% |
| wire-matrix-150x8 | toon-ext-all | 7628 | 5107 | 6.3% |
| wire-tree3-100 | json-minified | 37076 | 13370 | 0.0% |
| wire-tree3-100 | json-pretty | 90728 | 23165 | 73.3% |
| wire-tree3-100 | yaml | 64999 | 19319 | 44.5% |
| wire-tree3-100 | xml | 55675 | 20122 | 50.5% |
| wire-tree3-100 | toon-v3.3-canonical | 37889 | 13556 | 1.4% |
| wire-tree3-100 | toon-ext-primitive-array-columns | 37889 | 13556 | 1.4% |
| wire-tree3-100 | toon-ext-child-tables | 19076 | 8834 | -33.9% |
| wire-tree3-100 | toon-ext-delimiter-pipe | 38216 | 14354 | 7.4% |
| wire-tree3-100 | toon-ext-keyed-map-collapse | 37889 | 13556 | 1.4% |
| wire-tree3-100 | toon-ext-cyclic-discriminated-arrays | 37889 | 13556 | 1.4% |
| wire-tree3-100 | toon-ext-all | 19076 | 8834 | -33.9% |
| wire-honesty-non-uniform-rows | json-minified | 72 | 34 | 0.0% |
| wire-honesty-non-uniform-rows | json-pretty | 186 | 73 | 114.7% |
| wire-honesty-non-uniform-rows | yaml | 105 | 53 | 55.9% |
| wire-honesty-non-uniform-rows | xml | 134 | 61 | 79.4% |
| wire-honesty-non-uniform-rows | toon-v3.3-canonical | 95 | 48 | 41.2% |
| wire-honesty-non-uniform-rows | toon-ext-primitive-array-columns | 95 | 48 | 41.2% |
| wire-honesty-non-uniform-rows | toon-ext-child-tables | 95 | 48 | 41.2% |
| wire-honesty-non-uniform-rows | toon-ext-delimiter-pipe | 96 | 49 | 44.1% |
| wire-honesty-non-uniform-rows | toon-ext-keyed-map-collapse | 95 | 48 | 41.2% |
| wire-honesty-non-uniform-rows | toon-ext-cyclic-discriminated-arrays | 95 | 48 | 41.2% |
| wire-honesty-non-uniform-rows | toon-ext-all | 95 | 48 | 41.2% |
| wire-honesty-non-uniform-map | json-minified | 92 | 28 | 0.0% |
| wire-honesty-non-uniform-map | json-pretty | 154 | 53 | 89.3% |
| wire-honesty-non-uniform-map | yaml | 101 | 36 | 28.6% |
| wire-honesty-non-uniform-map | xml | 136 | 43 | 53.6% |
| wire-honesty-non-uniform-map | toon-v3.3-canonical | 92 | 30 | 7.1% |
| wire-honesty-non-uniform-map | toon-ext-primitive-array-columns | 92 | 30 | 7.1% |
| wire-honesty-non-uniform-map | toon-ext-child-tables | 92 | 30 | 7.1% |
| wire-honesty-non-uniform-map | toon-ext-delimiter-pipe | 92 | 30 | 7.1% |
| wire-honesty-non-uniform-map | toon-ext-keyed-map-collapse | 92 | 30 | 7.1% |
| wire-honesty-non-uniform-map | toon-ext-cyclic-discriminated-arrays | 92 | 30 | 7.1% |
| wire-honesty-non-uniform-map | toon-ext-all | 92 | 30 | 7.1% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | json-minified | 174 | 56 | 0.0% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | json-pretty | 343 | 110 | 96.4% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | yaml | 244 | 93 | 66.1% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | xml | 319 | 107 | 91.1% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | toon-v3.3-canonical | 184 | 72 | 28.6% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | toon-ext-primitive-array-columns | 109 | 48 | -14.3% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | toon-ext-child-tables | 184 | 72 | 28.6% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | toon-ext-delimiter-pipe | 187 | 76 | 35.7% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | toon-ext-keyed-map-collapse | 184 | 72 | 28.6% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | toon-ext-cyclic-discriminated-arrays | 184 | 72 | 28.6% |
| wire-extension-primitive-list-columns-decode-empty-lists-and-quoted-sub-delimiters | toon-ext-all | 109 | 48 | -14.3% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | json-minified | 322 | 108 | 0.0% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | json-pretty | 757 | 198 | 83.3% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | yaml | 525 | 157 | 45.4% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | xml | 508 | 171 | 58.3% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | toon-v3.3-canonical | 348 | 120 | 11.1% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | toon-ext-primitive-array-columns | 348 | 120 | 11.1% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | toon-ext-child-tables | 207 | 89 | -17.6% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | toon-ext-delimiter-pipe | 352 | 127 | 17.6% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | toon-ext-keyed-map-collapse | 348 | 120 | 11.1% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | toon-ext-cyclic-discriminated-arrays | 348 | 120 | 11.1% |
| wire-extension-recursive-child-tables-decode-per-row-child-counts | toon-ext-all | 207 | 89 | -17.6% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | json-minified | 54 | 24 | 0.0% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | json-pretty | 151 | 54 | 125.0% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | yaml | 79 | 37 | 54.2% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | xml | 117 | 47 | 95.8% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | toon-v3.3-canonical | 65 | 34 | 41.7% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | toon-ext-primitive-array-columns | 65 | 34 | 41.7% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | toon-ext-child-tables | 39 | 26 | 8.3% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | toon-ext-delimiter-pipe | 67 | 36 | 50.0% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | toon-ext-keyed-map-collapse | 65 | 34 | 41.7% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | toon-ext-cyclic-discriminated-arrays | 65 | 34 | 41.7% |
| wire-extension-mixed-empty-child-arrays-decode-zero-as-child-count | toon-ext-all | 39 | 26 | 8.3% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | json-minified | 28 | 17 | 0.0% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | json-pretty | 98 | 41 | 141.2% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | yaml | 67 | 39 | 129.4% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | xml | 140 | 53 | 211.8% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | toon-v3.3-canonical | 41 | 28 | 64.7% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | toon-ext-primitive-array-columns | 41 | 28 | 64.7% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | toon-ext-child-tables | 38 | 25 | 47.1% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | toon-ext-delimiter-pipe | 44 | 31 | 82.4% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | toon-ext-keyed-map-collapse | 41 | 28 | 64.7% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | toon-ext-cyclic-discriminated-arrays | 41 | 28 | 64.7% |
| wire-extension-matrix-rows-decode-as-uniform-fixed-width-lists | toon-ext-all | 38 | 25 | 47.1% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | json-minified | 912 | 298 | 0.0% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | json-pretty | 1552 | 538 | 80.5% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | yaml | 1137 | 456 | 53.0% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | xml | 1366 | 478 | 60.4% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toon-v3.3-canonical | 1000 | 393 | 31.9% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toon-ext-primitive-array-columns | 1000 | 393 | 31.9% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toon-ext-child-tables | 1000 | 393 | 31.9% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toon-ext-delimiter-pipe | 1001 | 394 | 32.2% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toon-ext-keyed-map-collapse | 1000 | 393 | 31.9% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toon-ext-cyclic-discriminated-arrays | 482 | 237 | -20.5% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toon-ext-all | 482 | 237 | -20.5% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toon-rust-ext-cyclic-discriminated-arrays | 482 | 237 | -20.5% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | jsonl | 900 | 305 | 2.3% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | csv | 368 | 177 | -40.6% |
| wire-extension-cyclic-three-event-cycle-with-common-prefix | toonl | 788 | 349 | 17.1% |

