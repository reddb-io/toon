# tq

A command-line query tool for TOON documents — the `jq`/`yq` equivalent for the TOON format, focused on speed and low memory/CPU footprint.

## Language

**TOON**:
The public Token-Oriented Object Notation spec (toon-format.dev); `tq` targets strict adherence to it, not an internal dialect.
_Avoid_: "reddb TOON", internal supersets

## Relationships

- **tq** parses and queries **TOON** documents, analogous to `jq` for JSON and `yq` for YAML.
- **tq** also converts bidirectionally between **TOON** and JSON (`-p json` input, `-o json` output; TOON in/out is the default on both sides).

## Flagged ambiguities

- (none yet)
