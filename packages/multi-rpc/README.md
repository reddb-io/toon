# @reddb-io/multi-rpc

One RPC method registry for JSON-RPC 2.0 and TOON-RPC 1.0. Requests are
detected per message and responses use the same wire format as the caller.

> **Recovery status:** this package is an experimental prototype with known
> protocol detection and validation gaps. Publication is paused under
> [the 0.30 recovery](https://github.com/reddb-io/toon/issues/389). Do not use
> the previously published 0.29 line in production.

## Installation

```bash
pnpm add @reddb-io/multi-rpc
```

## Usage

```typescript
import { MultiRpc, Server } from '@reddb-io/multi-rpc';

const server = new Server();
server.register('add', async ([a, b]) => a + b);

const multi = new MultiRpc(server);

await multi.handle('{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}');
// {"jsonrpc":"2.0","result":5,"id":1}

await multi.handle('toonrpc: "1.0"\nmethod: add\nparams[2]: 2,3\nid: 1');
// toonrpc: "1.0" / result: 5 / id: 1
```

The package also exports `detectProtocol`, `contentTypeFor`, `encodeMessage`,
`decodeMessage`, and their associated TypeScript types.

## License

MIT
