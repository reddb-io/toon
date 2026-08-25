# MCP conformance

## Pinned revision: `2026-07-28`

Both the Rust crate `reddb-io-toon-rpc-mcp` and the npm package
`@reddb-io/toon-rpc-mcp` target MCP revision **`2026-07-28`**, verified against
<https://modelcontextprotocol.io/specification/2026-07-28/> on 2026-08-24.

`2026-07-28` is the **current** revision per the
[versioning page](https://modelcontextprotocol.io/specification/versioning).
Issue #412 proposed pinning `2025-06-18`; that would pin a superseded revision,
so the newer stable one was verified and pinned instead, as the issue permits.

### What changed at this revision

`2026-07-28` removed the connection-establishing handshake. Version, client
identity, and client capabilities are now **per-request `_meta`**, and servers
**MUST** implement `server/discover`. Revisions `2025-11-25` and earlier — which
open with `initialize` — are termed **legacy** by the spec.

This matters for reading the code: `server/discover`, `resultType`, `ttlMs`, and
`cacheScope` are *schema fields of the pinned revision*, not local inventions.

## Method surface

| Method | Result key | Citation |
| --- | --- | --- |
| `server/discover` | `supportedVersions`, `capabilities`, `_meta` | [server/discover](https://modelcontextprotocol.io/specification/2026-07-28/server/discover) |
| `tools/list` | `tools` | [tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) |
| `tools/call` | `content`, `isError?`, `structuredContent?` | [tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) |
| `resources/list` | `resources` | [resources](https://modelcontextprotocol.io/specification/2026-07-28/server/resources) |
| `resources/read` | `contents` | [resources](https://modelcontextprotocol.io/specification/2026-07-28/server/resources) |
| `prompts/list` | `prompts` | [prompts](https://modelcontextprotocol.io/specification/2026-07-28/server/prompts) |
| `prompts/get` | `messages` | [prompts](https://modelcontextprotocol.io/specification/2026-07-28/server/prompts) |
| `ping` | `{}` | [utilities/ping](https://modelcontextprotocol.io/specification/2026-07-28/basic/utilities/ping) |

## Error codes

| Code | Meaning | Where used |
| --- | --- | --- |
| `-32700` | Parse error | A line that is not valid JSON |
| `-32600` | Invalid Request | Missing `method`, wrong `jsonrpc`, non-object message |
| `-32601` | Method not found | Unknown method; `initialize` in modern-only mode |
| `-32602` | Invalid params | Bad arguments, unknown tool, missing resource or prompt |
| `-32603` | Internal error | Serialization or unexpected failure |
| `-32022` | `UnsupportedProtocolVersionError` | `_meta` names a version not served |

A missing resource is `-32602` per the resources page; `-32002` is accepted by
clients only for backward compatibility, and is never emitted here.

**Tool failures are not JSON-RPC errors.** Actionable failures (bad input,
business rules, upstream errors) return a normal result with `isError: true`, so
the model can self-correct. Only protocol-level problems — an unknown tool, a
malformed request — raise a JSON-RPC error.

## Transports

### stdio — fully implemented

One JSON-RPC message per line, per the
[stdio binding](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio):

- messages are newline-delimited and **MUST NOT** contain embedded newlines
  (asserted in tests: `serde_json` and `JSON.stringify` escape control
  characters, so a newline inside a string never reaches the wire raw);
- nothing but MCP messages is written to stdout; logging goes to stderr;
- EOF on stdin ends the loop, which is the graceful shutdown signal.

### HTTP — POST only, Rust crate only

`serve_http_post` implements `POST /mcp` request/response as `application/json`.
It is **a subset of Streamable HTTP**, not the whole transport: there is no SSE,
no `subscriptions/listen`, no resumability, and no session headers. `GET /mcp`
returns 404 rather than an SSE content type. Clients that need server-initiated
messages must use stdio.

The npm package ships **no HTTP transport at all**.

## Backward compatibility with legacy clients

By default both implementations are **modern-only**: `initialize` is rejected
with `-32601`, and the error `data.supported` names the versions served. The
spec asks for exactly this, because a legacy client has no fall-forward
mechanism and this message may be its only diagnostic.

Opting in to dual-era — `McpDispatcher::with_legacy_initialize(true)` in Rust,
`{ legacyInitialize: true }` in TypeScript — also answers `initialize` with
`protocolVersion: "2025-11-25"` and advertises both versions from
`server/discover`. Modern behavior is unchanged; the option only adds a reply
for clients that open with a handshake.

## Not implemented

Declared here so no caller infers support from silence:

- Streamable HTTP as a whole (SSE, sessions, resumability, `MCP-Session-Id`).
- `subscriptions/listen` and every `notifications/*/list_changed` emission.
- Multi round-trip requests: `resultType: "input_required"`, `inputRequests`,
  `requestState`. Only `"complete"` is ever emitted.
- Pagination: `cursor` is accepted in params but never honored, and
  `nextCursor` is never emitted, so every list is a single complete page.
- `resources/templates/list`, completion, elicitation, sampling, roots, logging.
- Authorization, `x-mcp-header`, and icons.
- Client-side implementations. These are servers only.

## Relationship to TOON-RPC

Spec #389 §9: TOON-RPC extensions **MUST NOT** be presented as standard MCP
wire. MCP here is plain JSON-RPC 2.0 over `serde_json` / `JSON.parse`. Neither
implementation depends on the TOON codec or the TOON-RPC dispatcher, and both
test suites assert that a `{"toonrpc":"1.0",...}` message is rejected as an
Invalid Request.

## Verifying

```sh
cargo test -p reddb-io-toon-rpc-mcp          # 43 tests
npx -y pnpm@11.6.0 --filter @reddb-io/toon-rpc-mcp test   # 43 tests
```

Both suites drive scripted client transcripts — `server/discover` → `tools/list`
→ `tools/call` — through the real stdio loop, and assert each wire shape against
the values transcribed from the pages cited above.

### Compatibility claims

Conformance to the shapes cited here is what the tests prove. Neither
implementation has been exercised against a third-party MCP client, so no claim
of compatibility with any specific host is made. Note that as of this pin, hosts
in the field predominantly speak the legacy `initialize` era; reaching one needs
`legacyInitialize` enabled, and that path is verified only by our own transcript
tests.
