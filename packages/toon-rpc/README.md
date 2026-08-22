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

## License

MIT
