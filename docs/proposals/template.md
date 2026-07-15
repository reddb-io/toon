# Proposal — <Title>

**Stage:** <0 idea · 1 measured proposal · 2 frozen grammar · 3 implemented opt-in · 4 graduated>
**Status:** <one line — e.g. "graduated into toon-reddb-spec.md" / "prototype only">
**Spec section:** [<section title>](../toon-reddb-spec.md#<anchor>) *(once graduated)*
**Upstream RFC:** [toon-format/spec#NN](https://github.com/toon-format/spec/issues/NN) *(if any)*
**Repo issues / PRs:** #NN, #NN

> Copy this file to `docs/proposals/<kebab-case-name>.md` and fill every section.
> A proposal tells the whole story: what problem it solves, how the grammar
> works, how to test it, the measured numbers, why it is a good decision, and
> the trail of issues/PRs that got it there. Delete these blockquote lines and
> the angle-bracket placeholders when you write a real proposal.

## Motivation

<What problem in TOON v3.3 does this solve? What can't be expressed or checked
without it? Who feels the pain?>

## Design / grammar

<The exact wire grammar, with a worked TOON example and the JSON it decodes to.
State the eligibility rules an encoder uses, and the fail-closed behavior
against a strict v3.3 decoder. Include the expanded v3.3-equivalent form.>

## How to test it

<The flags/APIs that enable it on each surface (JS, Rust, `tq`), and the
fixtures / golden tests / commands that exercise it. Give a runnable command.>

## Measured numbers

<Tokens and bytes versus JSON and plain TOON v3.3, with the corpus and
tokenizer named. Numbers must be reproducible from the repo's harness.>

## Why it is a good decision

<The trade-off analysis: what it costs, what it buys, and any deliberately
accepted weakness (e.g. a weaker guardrail). Be honest about where it does
*not* help.>

## Stage transitions

- **Stage 0 — idea:** <link / date>
- **Stage 1 — measured proposal:** <link / date>
- **Stage 2 — frozen grammar:** <link / date>
- **Stage 3 — implemented opt-in:** <link / date>
- **Stage 4 — graduated:** <link to spec section / date>

## Links

- Spec section: [<title>](../toon-reddb-spec.md#<anchor>)
- Upstream RFC: <url>
- Repo issues / PRs: <urls>
