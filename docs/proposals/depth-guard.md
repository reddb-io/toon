# Proposal — Depth guard

**Stage:** 4 — graduated
**Status:** graduated into `toon-reddb-spec.md`. Robustness feature; changes no decoded value.
**Spec section:** [Depth guard](../toon-reddb-spec.md#depth-guard)
**Upstream RFC:** —
**Repo issues / PRs:** —

## Motivation

Neither the official spec's data model nor its strict-mode checklist bounds
nesting depth. A maliciously or accidentally deep document can drive a naïve
recursive decoder into **stack exhaustion** and crash the process. That is a
denial-of-service hazard for any service that decodes untrusted TOON — and TOON
is frequently produced by LLMs, whose output is exactly the "untrusted input"
case.

## Design / grammar

No wire-format change. A **depth guard** bounds recursion during decode and
checked encode:

- Decoding is bounded by `ParseOptions::max_depth` (Rust) and the equivalent JS
  parse option; checked encoding is bounded by `EncodeOptions::max_depth`.
- **Both default to `1000`.** A document nested deeper than the guard is rejected
  with a structured error rather than crashing.
- Setting `max_depth` to `0` disables the guard and MUST be done **only for
  trusted input**.
- On encode, prefer the checked entry points (`try_to_canonical_toon()` /
  `try_to_toon_with_options(...)` in Rust) for untrusted values, so a depth
  failure returns an `EncodeError` instead of overflowing.

Within the default limit, ordinary nesting decodes exactly as before:

```toon
a:
  b:
    c:
      value: 42
```

```json
{"a": {"b": {"c": {"value": 42}}}}
```

A pathologically deep document (1001+ levels) is rejected:

```
error: maximum nesting depth (1000) exceeded
```

## How to test it

- Rust decode: set `ParseOptions { max_depth, .. }`; encode: `EncodeOptions { max_depth, .. }` via the `try_*` entry points.
- JS: the equivalent parse option / encode option.
- Exercise both a within-limit document (decodes identically) and an
  over-limit document (structured error), plus `max_depth: 0` (guard disabled)
  on trusted fixtures.

## Measured numbers

There is nothing to measure on tokens or bytes — the guard never changes output.
Its "measurement" is behavioral: a within-limit document decodes byte-identically
whether or not the guard is present, and an over-limit document returns a
structured error instead of a stack overflow.

## Why it is a good decision

Defense-in-depth by default with an escape hatch. A conservative `1000` is far
above any legitimate hand- or machine-authored document yet well below the depth
that overflows a typical stack, so it protects real services without ever
tripping on real data. It is configurable for callers whose inputs are
known-shallow or known-trusted, and disabling it is a deliberate, documented
choice rather than a silent default. Because it is a limit and not a format
change, it composes with every extension for free.

## Stage transitions

- **Stage 0–1 — idea / measured proposal:** stack-exhaustion hazard on untrusted deep documents.
- **Stage 2 — frozen grammar:** no wire change; `max_depth` option with default `1000`.
- **Stage 3 — implemented opt-in:** always-on with configurable `max_depth`; checked encode entry points.
- **Stage 4 — graduated:** [Depth guard](../toon-reddb-spec.md#depth-guard).

## Links

- Spec section: [Depth guard](../toon-reddb-spec.md#depth-guard)
- Related: [detectTruncation](detect-truncation.md) is the other robustness/diagnostic feature over the same decoder.
