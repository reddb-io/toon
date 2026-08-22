# TOON-RPC Specification

**Version:** 0.1.0 (draft)
**Status:** Draft
**Supersedes:** JSON-RPC 2.0

## 1. Overview

TOON-RPC is a remote procedure call protocol that uses TOON (Token-Oriented Object Notation) as its serialization format, replacing JSON with a more efficient and human-readable alternative.

The protocol is transport-agnostic and designed to be simple, following the same semantics as JSON-RPC 2.0 with extensions for Server-Side Events and subscriptions.

## 2. Transport Independence

TOON-RPC is designed to work over any bidirectional streaming transport:

- **stdio** — Local IPC, CLI tools, debugging
- **HTTP** — Web APIs (stateless request/response)
- **TCP** — Network services, microservices
- **Unix Socket** — Local IPC with filesystem permissions
- **WebSocket** — Bidirectional streaming, real-time apps

Each transport implements the `Transport` trait providing send/recv streams.

## 3. Wire Protocol

All messages are valid TOON documents. The protocol reuses JSON-RPC 2.0 semantics with TOON serialization.

### 3.1 Request Object

```toon
{
  toonrpc: "1.0"
  method: "subtract"
  params: [42, 23]
  id: 1
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `toonrpc` | String | Yes | MUST be exactly `"1.0"` |
| `method` | String | Yes | Name of method to invoke. `rpc.*` prefix reserved. |
| `params` | Value | No | Method parameters (by-position Array or by-name Object) |
| `id` | String\|Number\|Null | No | If omitted = notification |

### 3.2 Notification

A Request without `id` is a notification. No response is sent.

```toon
{ toonrpc: "1.0" method: "update" params: [1, 2, 3, 4, 5] }
```

### 3.3 Response Object

**Success:**
```toon
{ toonrpc: "1.0" result: 19 id: 1 }
```

**Error:**
```toon
{ toonrpc: "1.0" error: { code: -32601 message: "Method not found" data: null } id: 1 }
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `toonrpc` | String | Yes | MUST be `"1.0"` |
| `result` | Value | Yes (success) | Method return value |
| `error` | Error | Yes (error) | Error object |
| `id` | Any | Yes | Matches request `id`, or `null` if parse error |

### 3.4 Error Object

```toon
{
  code: -32601
  message: "Method not found"
  data: null
}
```

| Field | Type | Description |
|-------|------|-------------|
| `code` | Integer | Error type (see below) |
| `message` | String | Short description (one sentence) |
| `data` | Value | Additional error info (optional) |

### 3.5 Error Codes

| Code | Message | Description |
|------|---------|-------------|
| -32700 | Parse error | Invalid TOON received |
| -32600 | Invalid Request | Not a valid Request object |
| -32601 | Method not found | Method does not exist |
| -32602 | Invalid params | Invalid method parameters |
| -32603 | Internal error | Internal RPC error |
| -32000 to -32099 | Server error | Implementation-defined |

### 3.6 Batch

```toon
[
  { toonrpc: "1.0" method: "sum" params: [1, 2, 4] id: "1" }
  { toonrpc: "1.0" method: "notify_hello" params: [7] }
  { toonrpc: "1.0" method: "subtract" params: [42, 23] id: "2" }
]
```

Response contains one Response per non-notification Request, in any order.

## 4. Extensions

### 4.1 Server-Side Events (SSE)

SSE allows servers to push events to clients over HTTP-like transports.

**Subscribe method:**
```toon
{ toonrpc: "1.0" method: "rpc.sse.subscribe" params: { event: "onUpdate" } id: 1 }
```

**Event push:**
```toon
{ toonrpc: "1.0" method: "rpc.sse.event" params: { event: "onUpdate" data: { value: 42 } } }
```

### 4.2 Subscriptions

Long-lived subscriptions with explicit unsubscribe.

**Subscribe:**
```toon
{ toonrpc: "1.0" method: "rpc.subscribe" params: { method: "events.onValue" } id: 1 }
```

**Response:**
```toon
{ toonrpc: "1.0" result: { subscriptionId: "abc123" } id: 1 }
```

**Unsubscribe:**
```toon
{ toonrpc: "1.0" method: "rpc.unsubscribe" params: { subscriptionId: "abc123" } id: 2 }
```

## 5. IDL (`.toonrpc`)

Service definitions in TOON format:

```toon
{
  version: "1.0"
  service: "Calculator"
  types: {
    Vec2: { x: f64 y: f64 }
  }
  methods: [
    { name: "add"      params: [{ a: i32 } { b: i32 }]      result: i32 }
    { name: "subtract" params: [{ a: i32 } { b: i32 }]      result: i32 }
    { name: "dot"      params: [{ v1: Vec2 } { v2: Vec2 }]  result: f64 }
  ]
  events: [
    { name: "onResult" payload: { value: i32 } }
  ]
}
```

### 5.1 Type System

| TOON-RPC Type | Description |
|---------------|-------------|
| `i8`, `i16`, `i32`, `i64` | Signed integers |
| `u8`, `u16`, `u32`, `u64` | Unsigned integers |
| `f32`, `f64` | Floating point |
| `bool` | Boolean |
| `string` | UTF-8 string |
| `bytes` | Binary data (base64) |
| `null` | Null value |
| `Object` | Named fields `{ field: Type }` |
| `Array` | Homogeneous list `[Type]` |
| `Vector` | Dynamic list `Type[]` |

## 6. Code Generation

Code generators produce:

- **Rust**: Server trait + client stub + types
- **TypeScript**: Client class + types

### 6.1 Generated Rust

```rust
// Generated from calculator.toonrpc
pub trait Calculator {
    fn add(&self, ctx: &RpcContext, a: i32, b: i32) -> RpcResult<i32>;
    fn subtract(&self, ctx: &RpcContext, a: i32, b: i32) -> RpcResult<i32>;
    fn dot(&self, ctx: &RpcContext, v1: Vec2, v2: Vec2) -> RpcResult<f64>;
}

#[derive(Serialize, Deserialize)]
pub struct Vec2 { pub x: f64, pub y: f64 }
```

### 6.2 Generated TypeScript

```typescript
// Generated from calculator.toonrpc
export interface Vec2 { x: number; y: number }

export class CalculatorClient {
  constructor(private transport: ToonRpcTransport)
  async add(a: number, b: number): Promise<number>
  async subtract(a: number, b: number): Promise<number>
  async dot(v1: Vec2, v2: Vec2): Promise<number>
}
```

## 7. Transport Trait

```rust
pub trait Transport {
    type Send: Stream<Item = Result<Bytes, Error>> + Unpin;
    type Recv: Stream<Item = Result<Bytes, Error>> + Unpin;

    fn split(self) -> (Self::Send, Self::Recv);
}
```

## 8. Examples

### 8.1 stdio

Server reads requests from stdin, writes responses to stdout.

### 8.2 HTTP

POST requests with TOON body, response in TOON.

### 8.3 WebSocket

Full-duplex channel with JSON-RPC-like framing over WebSocket frames.

## 9. Implementation Status

| Component | Status |
|-----------|--------|
| Core Protocol | Planned |
| stdio Transport | Planned |
| HTTP Transport | Planned |
| TCP/Unix Socket | Planned |
| WebSocket + SSE | Planned |
| Rust Codegen | Planned |
| TypeScript Codegen | Planned |
| CLI | Planned |

## 10. Differences from JSON-RPC

| Aspect | JSON-RPC | TOON-RPC |
|--------|----------|----------|
| Serialization | JSON | TOON |
| Human-readable | Moderate | Better (no quotes on keys) |
| Type system | JSON types | Extended with bytes, u64, i64 |
| Streaming | Not specified | Via SSE extension |
| Transport | HTTP only (commonly) | Any bidirectional stream |
