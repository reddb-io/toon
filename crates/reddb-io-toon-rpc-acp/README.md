# reddb-io-toon-rpc-acp

> **LEGACY / TERMINAL.** Frozen contract. No new features.

An HTTP server for **this repository's own legacy ACP-style REST contract**.

## What this is not

This crate does **not** implement, and is **not** interoperable with:

- IBM/BeeAI's **Agent Communication Protocol**
- Zed's **Agent Client Protocol**

The resemblance is nominal only. The run envelope (`agentRunId`, `agentName`),
the message-part model (`kind` / `status`) and the run-status vocabulary
(`created`, `in_progress`, `awaiting`, …) were invented in this repository and
match no published protocol.

## The pinned contract

The wire shapes are pinned by [`docs/acp-legacy-openapi.yaml`](../../docs/acp-legacy-openapi.yaml)
at the repository root. That document is the contract of record for both this
crate and `@reddb-io/toon-rpc-acp`.

The contract is terminal: it accepts correctness, safety and lifecycle fixes
that keep the documented shapes byte-identical, and nothing else. No new
endpoints, fields, status values or part kinds will be added. New
agent-protocol work belongs on a different, non-legacy surface.

## Endpoints

| Method   | Path                   | Behaviour                                          |
| -------- | ---------------------- | -------------------------------------------------- |
| `GET`    | `/`                    | Service descriptor.                                |
| `GET`    | `/agents`              | Agent summaries.                                   |
| `POST`   | `/agents/{name}/runs`  | Run the agent, retain and return the run (200).    |
| `GET`    | `/runs/{id}`           | Read a retained run; reading does not consume it.  |
| `DELETE` | `/runs/{id}`           | Cancel a live run, or release a finished one.      |

`Accept: application/toon` switches every response to TOON; anything else
yields JSON.

## Lifecycle notes

- `AcpService::run` is synchronous and may block for the whole length of a
  run. The transport calls it on tokio's blocking pool, never on an async
  worker, so one slow run does not stall unrelated connections.
- Runs are retained in a bounded `RunStore` (`AcpHttpConfig::max_runs`,
  default 1024). Finished runs are evicted before live ones.
- `DELETE` on a run in a terminal state (`completed`, `failed`, `cancelled`)
  releases it without consulting `AcpService::cancel`, so a finished run can
  always be removed even though the default `cancel` hook fails.

## Example

```bash
cargo run --example agent_server -p reddb-io-toon-rpc-acp
curl -H 'Accept: application/toon' http://127.0.0.1:9000/agents
```
