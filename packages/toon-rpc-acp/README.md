# @reddb-io/toon-rpc-acp

> **LEGACY / TERMINAL.** Frozen contract. No new features.

A client for **this repository's own legacy ACP-style REST contract**, served
by the `reddb-io-toon-rpc-acp` crate.

## What this is not

This package does **not** implement, and is **not** interoperable with:

- IBM/BeeAI's **Agent Communication Protocol**
- Zed's **Agent Client Protocol**

The run envelope (`agentRunId`, `agentName`), the message-part model
(`kind` / `status`) and the run-status vocabulary were invented in this
repository and match no published protocol.

## The pinned contract

The wire shapes are pinned by [`docs/acp-legacy-openapi.yaml`](../../docs/acp-legacy-openapi.yaml)
at the repository root, and are frozen. Only correctness, safety and lifecycle
fixes that keep those shapes byte-identical land here.

## Usage

```ts
import { callAgent, listAgents } from '@reddb-io/toon-rpc-acp';

const agents = await listAgents('http://127.0.0.1:9000');

const run = await callAgent(
  'http://127.0.0.1:9000',
  'echo',
  [{ kind: 'text', content_type: 'text/plain', content: 'hello', status: 'done' }],
  { timeoutMs: 5_000 },
);
```

### Options

| Option      | Effect                                                                                                 |
| ----------- | ------------------------------------------------------------------------------------------------------ |
| `toon`      | Send **and** parse TOON: switches the request body, `Content-Type`, `Accept` and the response parser together. Default is JSON on both sides. |
| `signal`    | Caller-supplied `AbortSignal`, forwarded to `fetch`.                                                    |
| `timeoutMs` | Abort the request after this many milliseconds; composes with `signal`.                                 |
