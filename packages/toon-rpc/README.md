# @reddb-io/toon-rpc

TOON-RPC client and server. JSON-RPC 2.0 semantics with TOON serialization.

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
error: null
id: 1

# Error
toonrpc: "1.0"
result: null
error: { code: -32601 message: "Method not found" data: null }
id: 1
```

## Error Codes

Standard JSON-RPC 2.0 error codes:

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
