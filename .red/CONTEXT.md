# tq

A command-line query tool for TOON documents — the `jq`/`yq` equivalent for the TOON format, focused on speed and low memory/CPU footprint.

## Language

**TOON**:
The public Token-Oriented Object Notation spec (toon-format.dev); `tq` targets strict adherence to it, not an internal dialect. Baseline: spec v4.1.
_Avoid_: "reddb TOON", internal supersets

**Proposal**:
A design-history document under `docs/proposals/`. Once the mechanism it describes is absorbed by the official spec, the proposal is documentation only — it carries no normative weight and defines no alternative syntax.
_Avoid_: treating a proposal as a live dialect feature

**Checkpoint**:
The pinned commits of the `vendor/toon` and `vendor/toon-spec` submodules — the exact upstream state our conformance claims refer to.

## Relationships

- **tq** parses and queries **TOON** documents, analogous to `jq` for JSON and `yq` for YAML.
- **tq** also converts bidirectionally between **TOON** and JSON (`-p json` input, `-o json` output; TOON in/out is the default on both sides).

## Flagged ambiguities

- (none yet)
