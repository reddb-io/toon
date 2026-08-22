# @reddb-io/toon-rpc

TOON-RPC client and server. JSON-RPC 2.0 semantics with TOON serialization.

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
