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
