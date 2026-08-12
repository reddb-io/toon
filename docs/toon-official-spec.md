# Official TOON v4.1 baseline

This repository implements the released
[TOON v4.1.1 specification](https://github.com/toon-format/spec/releases/tag/v4.1.1)
as its canonical document format. The upstream specification is authoritative;
this page records the exact revision, local API boundaries, and executable
evidence. It does not define another dialect or copy proposed syntax into the
released baseline.

## Pinned release and evidence

The released checkpoint is `toon-format/spec` revision `62f16b3` and reference
implementation revision `a9e6d97`. Both are present in the `vendor/toon-spec`
and `vendor/toon` submodules. Run the public conformance paths from a checkout
with initialized submodules:

```sh
node --test packages/toon/test/conformance.test.mjs
cargo test -p reddb-io-toon --test spec_conformance
```

The commands read `vendor/toon-spec/tests/fixtures` directly. Expected-failure
ledgers are explicit ratchets, so the evidence is the command and fixture
revision—not an unqualified parity percentage. The complete workspace gates
remain `pnpm -r test` and `cargo test --workspace`.

## API boundaries

| Surface | Canonical v4.1 | Streaming | Experimental |
| --- | --- | --- | --- |
| TypeScript | `decode`, `decodeFromLines`, `decodeStream`, `encode`, `encodeLines` | TOONL v0.2 helpers and event decoders | `decode` reviver from the pinned upstream PR head |
| Rust | `decode*`, `encode*`, `Value::parse_toon`, `Document::parse`, and canonical output methods | TOONL v0.2 readers, writers, and transforms | none |
| `tq` | v4.1 decode and output by default, plus `--no-strict` | TOONL query, conversion, check, trim, and close | none |
| Editor | v4.1 grammar, including full-line comments, nested field groups, and keyed tabular form; no semantic decoder | TOONL v0.1/v0.2 grammar | highlights local extension forms without presenting them as official |

There is one codec. The pre-v4 engine and the API that reached it — the
`@reddb-io/toon/legacy` subpath and Rust's `Legacy*` types and `*_legacy`
methods — are gone.

The opt-in wire extensions are defined in
[`toon-reddb-spec.md`](toon-reddb-spec.md). TOONL is independently versioned in
[`toonl-reddb-spec.md`](toonl-reddb-spec.md). Neither is official TOON syntax.

## Official frontier status

Status was **audited on 2026-08-07** against
[the machine-readable checkpoint](../.github/upstream-watch.json). “Official”
means released specification text, not merely an open issue, a branch commit,
or a local implementation.

| State | Item | Local disposition |
| --- | --- | --- |
| **Released** | TOON v4.1.1 spec at `62f16b3` and reference package at `a9e6d97` | Canonical baseline and conformance evidence pin. |
| **Merged**, unreleased | `toon-format/toon` default-branch HEAD `f06ddca` is ahead of the released implementation pin | Monitor only; a merged branch commit does not redefine v4.1.1. |
| **Draft** and **conflicting** | [spec PR #47](https://github.com/toon-format/spec/pull/47), mixed columnar arrays | Do not describe its spill-line syntax as released. |
| Open RFC | [spec issue #48](https://github.com/toon-format/spec/issues/48), mixed columnar arrays and omission controls | Evidence discussion only; no released mapping to a local extension. |
| Experimental implementation | [toon PR #294](https://github.com/toon-format/toon/pull/294), decoder reviver | TypeScript-only opt-in pinned to the audited PR head; not normative. |
| **Rejected** local direction | [broad discriminated/heterogeneous arrays](proposals/discriminated-heterogeneous-arrays.md) | Design history; do not implement as proposed. |
| **Userland-only** | TOONL, primitive-array columns, object-array columns, cyclic discriminated arrays, depth guard, and truncation reports | RedDB APIs/extensions; never label them official TOON. |

The bounded drift procedure and meanings of release, HEAD, issue, PR, draft,
and conflict movement are documented in
[`upstream-monitoring.md`](upstream-monitoring.md). The current mixed-columnar
assessment, including the distinct role of the completed v4 roadmap, is in
[`upstream-feedback.md`](upstream-feedback.md).

## Migration

The approved TypeScript and Rust call-site cutovers, removed behavior, and
before/after observable output are in [`migration-v4.md`](migration-v4.md).
