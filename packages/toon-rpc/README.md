# @reddb-io/toon-rpc

TOON-RPC 1.0 client and server. TOON-RPC borrows JSON-RPC's envelope model but
is a separate wire protocol encoded as UTF-8 TOON.

> **Recovery status:** this package is an experimental prototype with known
> protocol, client, and transport correctness gaps. Publication is paused under
> [the 0.30 recovery](https://github.com/reddb-io/toon/issues/389). Do not use
> the previously published 0.29 line in production.

## Installation

```bash
pnpm add @reddb-io/toon-rpc @reddb-io/toon
```

## Usage

### Client

```typescript
import { Client, createStdioTransport } from '@reddb-io/toon-rpc';

const transport = createStdioTransport();
const client = new Client(transport);

const result = await client.call('sum', [1, 2]);
console.log(result); // 3
```

### Server

```typescript
import { Server } from '@reddb-io/toon-rpc';

const server = new Server();

server.register('sum', async (params) => {
  return params[0] + params[1];
});

// Handle incoming requests
const response = await server.handle(inputData);
```

## Wire Format

TOON-RPC uses TOON v4.1 for serialization:

```toon
# Request
toonrpc: "1.0"
method: sum
params[2]: 1,2
id: 1

# Response
toonrpc: "1.0"
result: 3
id: 1

# Error
toonrpc: "1.0"
error:
  code: -32601
  message: "Method not found"
id: 1
```

Success responses contain `result` and no `error`; error responses contain
`error` and no `result`. `result: null` is a successful result. Error `data` is
optional, and an explicit `data: null` is preserved. Only an absent request
`id` is a notification; `id: null` is a request. Omitted `params` stay omitted,
while present `params` must be an array or object.

Runtime validators and snapshots reject present members whose value is
`undefined`, including `data: undefined`. TypeScript projects that do not enable
`exactOptionalPropertyTypes` can still construct that optional-property shape,
so it must not be treated as runtime-valid; the `RpcError` constructor overloads
reject an explicit third `undefined` argument under `strict` typing.
With `exactOptionalPropertyTypes`, explicit `undefined` is also rejected for
optional envelope members. The exported `CoreValue` type excludes `undefined`
recursively and accepts readonly arrays, while the
`snapshotCoreValue`, `snapshotRequestObject`, and `snapshotResponse` helpers
validate own data properties and return stable local containers.
Snapshots reject cycles, materialize acyclic aliases as independent
wire-equivalent copies, and use a fixed defensive expansion budget to bound
large DAGs and hostile expanding Proxies. Avoiding redundant work for repeated
aliases is a future resource-slice optimization; configurable protocol limits
remain deferred to the limits work in slice 11.
The budget bounds processing after an `ownKeys` result is returned; JavaScript
cannot interrupt a blocking `ownKeys` Proxy trap itself.

`Server.dispatchEntry` performs codec-independent validation and dispatch.
`Server.handle` and `handleText` additionally preflight generated responses in
their final TOON root context before emission.

## Error Codes

TOON-RPC reserved error codes:

| Code | Message |
|------|---------|
| -32700 | Parse error |
| -32600 | Invalid Request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |

## Transports

`@reddb-io/toon-rpc` is transport-agnostic. Implement the `Transport` interface:

```typescript
interface Transport {
  send(data: Uint8Array): Promise<void>;
  recv(): AsyncIterable<Uint8Array>;
  close(): Promise<void>;
}
```

A stdio transport is included via `createStdioTransport()`. For HTTP, WebSocket, and other transports, see the Rust packages: `@reddb-io/toon-rpc-http`, `@reddb-io/toon-rpc-ws`, etc.

## Publishing

```bash
# Build
pnpm build

# Dry-run
npm publish --dry-run

# Publish (requires npm login)
npm publish --access public
```

## License

MIT

## Multi-protocol: JSON-RPC 2.0 and TOON-RPC on one endpoint

Install the dedicated `@reddb-io/multi-rpc` package. `MultiRpc` wraps a
`Server` and answers each request in the dialect it arrived in — the same
detection rules as the Rust `reddb_io_toon_rpc::multi` module:

```ts
import { MultiRpc, Server } from '@reddb-io/multi-rpc';

const server = new Server();
server.register('add', async ([a, b]) => a + b);
const multi = new MultiRpc(server);

await multi.handle('{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}');
// → {"jsonrpc":"2.0","result":5,"id":1}
await multi.handle('toonrpc: "1.0"\nmethod: add\nparams[2]: 2,3\nid: 1');
// → toonrpc: "1.0" / result: 5 / id: 1
```

## dualDialectStream: an ndJsonStream that speaks both dialects

`dualDialectStream(output, input)` is signature-compatible with the ACP SDK's
`ndJsonStream` and returns the same `{ writable, readable }` object-stream
pair. Each inbound frame is sniffed on its own bytes (`{` opens a one-line
JSON frame; anything else is a TOON document terminated by a blank line), the
consumer always sees `jsonrpc: "2.0"` objects, and writes answer in the
dialect the peer last proved — so a JSON-RPC peer and a TOON-RPC peer can
share one socket with neither being configured.

```ts
import { dualDialectStream } from '@reddb-io/toon-rpc/acp-stream';

const stream = dualDialectStream(socketWritable, socketReadable, { preferred: 'toonrpc' });
```
