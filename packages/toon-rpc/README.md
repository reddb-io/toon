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
import { Client, type DuplexTransport } from '@reddb-io/toon-rpc';

declare const transport: DuplexTransport;
const client = new Client(transport, {
  onDiagnostic(diagnostic) {
    console.warn('Rejected RPC response', diagnostic);
  },
});

const result = await client.call('sum', [1, 2], {
  timeoutMs: 5_000,
});
console.log(result); // 3

await client.notify('telemetry.flush');
await client.close();
```

The client starts its receive pump lazily, validates each complete response
document, and owns pending-call cleanup. Calls may use generated numeric IDs or
an explicit string, safe-integer, or `null` ID. Abort, timeout, send failure,
transport failure, normal transport completion, and explicit close each settle
affected calls exactly once. `pendingCallCount` and `status` expose lifecycle
state without exposing mutable pending entries.

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

`@reddb-io/toon-rpc` separates framed duplex transports from direct
request/response transports:

```typescript
interface DuplexTransport {
  readonly kind: 'duplex';
  send(document: Uint8Array, options?: { signal?: AbortSignal }): Promise<void>;
  receive(options?: { signal?: AbortSignal }): AsyncIterable<Uint8Array>;
  close(): Promise<void>;
}

interface RequestResponseTransport {
  readonly kind: 'request-response';
  request(
    document: Uint8Array,
    options?: { signal?: AbortSignal }
  ): Promise<Uint8Array | undefined>;
  close(): Promise<void>;
}
```

Each duplex `receive()` item and each direct `request()` result is one complete
UTF-8 TOON document, not an arbitrary byte chunk. A direct transport returns
`undefined` only when no response document exists, such as for a notification;
it is never modeled as a duplex transport with a failing receive method. Its
response is scoped to that exchange and cannot settle another concurrent call.
If the direct exchange ends without the initiating call's response, that call
rejects with `ClientProtocolError`; the document's rejected entries still emit
diagnostics. Closing a duplex transport must terminate its `receive()` iterator,
and transports should honor the supplied abort signal for prompt cancellation.

Rejected malformed, unknown-ID, and duplicate-ID response entries are reported
through `ClientOptions.onDiagnostic`. Valid batch siblings still settle their
calls, and unmatched calls remain pending until a response, abort, timeout,
transport termination, or client close.

### Concrete transports

Every concrete transport implements one of the two contracts above and plugs
into `Client` directly:

| Subpath | Transport | Contract | Notes |
| --- | --- | --- | --- |
| `@reddb-io/toon-rpc/http` | `HttpTransport` | request/response | one POST per document; 204 or an empty body means no response document |
| `@reddb-io/toon-rpc/websocket` | `WebSocketTransport` | duplex | one complete document per text or binary frame; unsupported payloads fail deterministically; drives a browser `WebSocket` or the `ws` package via `options.webSocket` (or `createNodeWebSocketTransport`) |
| `@reddb-io/toon-rpc/tcp` | `TcpTransport` | duplex | length-prefixed stream framing; injectable socket factory |
| `@reddb-io/toon-rpc/stdio` | `StdioTransport` | duplex | length-prefixed stream framing over injectable stdin/stdout |
| `@reddb-io/toon-rpc/sse` | `SseTransport` | duplex | POST for outbound documents, one complete document per `data:` event inbound; built on fetch streaming, not `EventSource` |

```typescript
import { Client } from '@reddb-io/toon-rpc';
import { TcpTransport } from '@reddb-io/toon-rpc/tcp';

const client = new Client(new TcpTransport({ host: '127.0.0.1', port: 7333 }));
const sum = await client.call('sum', [1, 2]);
await client.close();
```

Byte-stream transports (TCP, stdio) speak the length-prefixed framing profile
from `@reddb-io/toon-rpc/framing` — `<decimal payload length>\n<payload>\n` —
never newline inference; `encodeFrame` and `FrameDecoder` are exported for
servers and other peers, and any framing violation fails the stream instead of
resynchronizing. The legacy 0.29 `Transport` shapes are gone.

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
pair, at ndJsonStream behavioral parity. Each inbound frame is sniffed on its
own bytes (`{` **or `[`** opens a one-line JSON frame — a `[` can only be a
JSON-RPC batch; anything else is a TOON document terminated by a blank line),
the consumer always sees `jsonrpc: "2.0"` objects (a batch arrives as an array
of them), and writes answer in the dialect the peer last **decoded** in — the
latch moves on proof, never on the framing sniff alone.

Parity rules, each of them load-bearing: a top-level array is always written
as one JSON line in either dialect (TOON cannot carry a root array as one
document); a malformed frame is reported through `onDiagnostic` and skipped,
never a torn-down connection; the final unterminated frame at end of input is
flushed; and cancelling the readable cancels the underlying byte reader.
`preferred` selects only the pre-proof opener: the default `"jsonrpc"` is the
only opener safe against a stock JSON-RPC peer, and `preferred: "toonrpc"` is
an explicit opt-in for closed deployments whose peers are known to read
TOON-RPC — a negotiated downgrade proof for open systems is future spec work.

```ts
import { dualDialectStream } from '@reddb-io/toon-rpc/acp-stream';

const stream = dualDialectStream(socketWritable, socketReadable, { preferred: 'toonrpc' });
```
