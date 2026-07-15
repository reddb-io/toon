# Proposal — Delimiter choice

**Stage:** 4 — graduated
**Status:** graduated into `toon-reddb-spec.md`. This is a *defaults and guidance* decision on top of a pure v3.3 mechanism — no grammar change, no fourth delimiter.
**Spec section:** [Delimiter choice](../toon-reddb-spec.md#delimiter-choice)
**Upstream RFC:** [toon-format/spec#48](https://github.com/toon-format/spec/issues/48)
**Repo issues / PRs:** —

## Motivation

TOON v3.3 already supports three delimiters — comma (default), tab (HTAB), and
pipe (`|`) — selected by the encoder as the *document delimiter* and declared
per array header as the *active delimiter*. What v3.3 does not settle is *when
to reach for a non-default*. Choosing badly leaves tokens on the table: a
comma-delimited table full of free-text cells pays per-cell quoting that a
tab-delimited table would avoid entirely.

## Design / grammar

No grammar change and **no fourth delimiter**. The reddb-io flavor only fixes
the defaults and the guidance:

- **Comma is the default**, matching the official spec: most familiar and most
  token-efficient when cells do not themselves contain commas.
- **Tab** when cells routinely contain commas (free-text, locale-formatted
  numbers): a comma value needs no quotes under a tab-delimited header, which
  usually nets fewer tokens than comma-plus-quotes.
- **Pipe** for human-facing tables and payloads whose cells contain neither
  pipes nor commas uniformly.

Comma-delimited (default), with one cell quoted because it contains a comma:

```toon
items[2]{id,description}:
  1,pen
  2,"eraser, pink"
```

Tab-delimited — cells with commas need no quoting:

```toon
data[2	]{value	note}:
  100	item, qty 5
  200	item, qty 3
```

Pipe-delimited — human-readable:

```toon
users[2|]{name|status}:
  Alice|active
  Bob|inactive
```

The flavor keeps the spec rule that **absence of a delimiter symbol always means
comma**, with no inheritance from a parent header, so a nested header's delimiter
is always locally legible. Delimiter selection never changes the decoded value.

## How to test it

- JS: `serialize(value, { delimiter: ',' | '\t' | '|' })`
- Rust: `to_toon_with_options(EncodeOptions { delimiter, .. })` with `',' | '\t' | '|'`
- `tq`: `--delimiter comma|tab|pipe`

The encoder emits the active-delimiter declaration in each header (`[N|]`,
`[N\t]`, matching field lists) and quotes cells containing the active delimiter.
Golden tests cover each delimiter across all three surfaces; the round-trip is
lossless for every choice.

## Measured numbers

The lever is purely wire-efficiency and readability. The win is data-dependent:
tab beats comma exactly when it removes more quote characters (and their tokens)
than the tab costs, which happens for comma-heavy free-text columns. There is no
universal constant — the guidance is a rule of thumb, and the spec's honest
advice is to measure your own payload before quoting a number.

## Why it is a good decision

Because delimiter choice never changes the decoded value, it is a free
efficiency/readability knob with zero compatibility risk: any delimiter is
already valid v3.3. Adding *guidance and defaults* rather than a new delimiter
keeps the flavor a strict subset-of-mechanism, so no reader anywhere needs new
capability. The one thing to avoid is treating it as magic: it only helps when
the delimiter you pick is genuinely rarer in the data than the one you replaced.

## Stage transitions

- **Stage 0–1 — idea / measured proposal:** delimiter mechanism from upstream RFC [toon-format/spec#48](https://github.com/toon-format/spec/issues/48).
- **Stage 2 — frozen grammar:** unchanged v3.3 delimiter mechanism; defaults fixed.
- **Stage 3 — implemented opt-in:** `delimiter` option / `--delimiter`.
- **Stage 4 — graduated:** [Delimiter choice](../toon-reddb-spec.md#delimiter-choice).

## Links

- Spec section: [Delimiter choice](../toon-reddb-spec.md#delimiter-choice)
- Upstream RFC: https://github.com/toon-format/spec/issues/48
