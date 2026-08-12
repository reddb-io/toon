# `tests/golden/toon-cli/` — the shared `toon` CLI corpus

One directory per invocation of the `toon` bin. Both front-ends run every case
and must produce the same bytes:

| Runner | Front-end |
|---|---|
| `tests/runners/rust/toon/cli_golden.rs` | the Rust bin, `crates/toon/src/bin/toon.rs` |
| `packages/toon/test/cli-golden.test.mjs` | the TypeScript bin, `packages/toon/bin/toon.mjs` |

A case that passes on both sides is byte parity between the two ports — the
contract Spec #359 asks for, over the one canonical event-stream engine.

## Case layout

| File | Required | Meaning |
|---|---|---|
| `args.txt` | yes | One argument per line. The trailing newline terminates the last argument; it does not add an empty one. An empty file means no arguments. |
| `stdin.txt` | no | The bytes on stdin. Absent means empty stdin. |
| `files/` | no | Files seeded into a throwaway working directory before the run. |
| `stdout.txt` | yes | Exact expected stdout. |
| `stderr.txt` | yes | Exact expected stderr. |
| `exit.txt` | yes | Expected exit code. |
| `output/` | no | Files the run must have written into the working directory, compared byte for byte. |

Each case runs in its own throwaway working directory, so `--output` writes and
the relative labels in the `✔ Encoded …` lines are real without a case reaching
into the repository.

## What the corpus deliberately leaves out

A case belongs here only when both front-ends can be held to the same bytes.
Three kinds of behaviour cannot be, and are pinned by the per-language suites
instead:

- **`--verbose`.** Upstream appends a JavaScript stack trace; Rust has no stack
  to append and carries the cause chain alone.
- **Messages that quote a host error.** A missing input file or malformed JSON
  reports what Node or `serde_json` said, and the two disagree by wording.
- **Codec diagnostics that disagree across ports.** A row-count mismatch reads
  `expected 3 tabular rows, but got 2` in TypeScript and `array count mismatch`
  in Rust. That divergence lives in the codec, not the front-end; the CLI
  renders whichever reason it is handed, and
  `error-decode-tab-indentation` pins that rendering with a reason both ports
  agree on.

## Regenerating

The corpus is authored, not generated: expected bytes come from running the
upstream-compatible TypeScript bin and reviewing the result. A case is added by
creating its directory, and it is retired by deleting it — both runners
discover cases from the directory listing.
