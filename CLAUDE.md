## Development workflow

- One-off concrete work goes through `/go "<demand>"` (ADR 0081): it mints a disposable `lane:go` issue, works in an isolated worktree under `.red/tmp/go-workers/`, runs the shared gate, and brings back a PR. Route the structured backlog through `/afk`; put a parked issue back in the queue with `/retake`.
- When working by hand instead (e.g. a slice the maintainer decided to land manually), work in an isolated worktree under `.red/tmp/work-*/`; do not create sibling worktrees outside the repo.
- Create task branches with `git worktree add .red/tmp/work-<slug> -b <branch> origin/main`, not with `git checkout -b` or `git switch -c` in the primary checkout.
- Commit the worktree, push the branch early, open a PR, monitor its checks, then merge it or park the issue/PR for `/hitl`.
- The agent never switches the primary checkout's branch; only the user does. With `plugins.dev.enabled: true`, the dev command proxy blocks agent-created worktrees outside `.red/tmp/` and primary-checkout branch movement.

## Agent skills

### Issue tracker

Issues and Specs live on GitHub Issues (reddb-io/tq) via the `gh` CLI. See `.red/agents/issue-tracker.md`.

### Triage labels

Canonical triage vocabulary (needs-triage, ready-for-agent, ready-for-human, …), default strings. See `.red/agents/triage-labels.md`.

### Domain docs

Single-context: `.red/CONTEXT.md` + `.red/adr/` at the repo root. See `.red/agents/domain.md`.
