# Own filter engine instead of embedding jaq

`tq` adopts jq-compatible query syntax (the familiarity that made yq succeed), and the fastest route to full compatibility would have been embedding jaq (a Rust implementation of the jq language) as the filter engine. We decided to build our own engine instead, starting as a growing subset of jq semantics. The reason: tq's reason to exist is being fast and memory/CPU-efficient on TOON specifically — e.g. skipping tabular-array rows without materializing a full value tree — and a value-tree engine like jaq forecloses those TOON-native optimizations at the core.

## Consequences

- v1 ships a jq subset, not full jq compatibility; the subset grows release by release.
- The engine can operate on TOON-aware representations (streaming, lazy tabular rows) rather than a mandatory materialized value tree.
