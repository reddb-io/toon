# Official upstream monitoring

The repository keeps an explicit, dated audit checkpoint in
`.github/upstream-watch.json`. It treats each official repository's latest
release and default-branch HEAD as different facts. The checkpoint also names
the local conformance evidence covered by the audit and records the state,
revision, disposition, and local impact of each watched issue or pull request.

Run the bounded, read-only live check with:

```sh
pnpm check:upstream
```

The command prints a dated Markdown report. Exit code `0` means the observed
state matches the checkpoint, `1` means maintainer review is needed, and `2`
means the observation could not be completed. A report separates release
movement, HEAD movement, and conformance-evidence movement. Watched frontiers
report issue updates, closure, merge, ordinary PR-head updates, force-pushes,
draft changes, and conflict changes together with the recorded local action.

The live collector performs a fixed set of GitHub REST reads. It resolves one
latest release and one branch HEAD per repository, reads only the listed
issues and pull requests, and compares ancestry only when a watched PR head
changed. It does not fetch repository content, update a submodule, change the
checkpoint, or import syntax.

For a deterministic offline observation, supply a captured snapshot and date:

```sh
pnpm check:upstream -- --snapshot path/to/snapshot.json --date 2026-08-07
```

CI runs `pnpm test:upstream` against in-memory fixtures, so parser and decision
coverage never depends on live API availability. A separate weekly workflow
runs the bounded live check and writes its report to the workflow summary.

After reviewing a report and rerunning the evidence named in it, a maintainer
may deliberately update `.github/upstream-watch.json`. Updating that file is
the only way to advance the audit checkpoint; the checker never repins either
submodule automatically.

## Cross-implementation differential run

On 2026-09-23 the two shipped engines were run through
[toon-diff](https://github.com/antrixy/toon-diff) (MIT; toon-format/toon
discussion #323), a differential tester that checks `decode_Y(encode_X(value))`
for every ordered pair of implementations against a lossless oracle, which
compares numbers by their exact source lexeme. The run used toon-diff's corpus
and mutation generator (13 seeds × 200 mutations on three generator seeds,
2,614 cases each) with a local driver outside this repository and three
engines: the upstream TypeScript reference at v4.1.1, `@reddb-io/toon`, and the
Rust `toon` CLI.

About 70,000 pair checks found **no structural disagreement** between the
three engines. The one class of finding was numeric: the Rust CLI's JSON
output rounded integers beyond `i64`/`u64` through `f64`. That is fixed, and the
remaining JSON-input limit is documented in the crate README. When an engine
ingests through JSON, the expectation is the `f64` reading, so the TypeScript
engines' documented `Number` domain is not reported.

The run is now a weekly workflow, `.github/workflows/toon-diff.yml` (Mondays,
and on demand). It builds the Rust `toon` CLI, checks out toon-diff at a
pinned revision, and copies in the driver from
[`scripts/toon-diff/reddb-matrix.ts`](../scripts/toon-diff/reddb-matrix.ts).
It then runs two generator seeds against the upstream TypeScript reference
pinned in the workflow. Numbers are compared exactly between the two Rust
engines, which keep integers of any size, and by their `f64` reading whenever
a JavaScript engine is involved. The driver exits non-zero on any finding and
writes its report to the workflow summary.
Like the drift check, it reports only and never gates a release. To run it
locally, do the same from a toon-diff checkout, with `REDDB_ROOT` pointing at
this repository and `REDDB_TOON_BIN` at a built `toon`.
