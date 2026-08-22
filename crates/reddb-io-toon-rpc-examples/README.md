# toon-rpc-examples

Calculator server and client examples showing TOON-RPC end-to-end.

## HTTP Example

### Server

```bash
cargo run --bin calculator_server
```

Output:
```
Calculator HTTP server listening on http://127.0.0.1:8080
```

### Client

In another terminal:
```bash
cargo run --bin calculator_client add 5 3
```

Output:
```
add 5 3 = 8
```

Or with `curl`:
```bash
curl -X POST http://127.0.0.1:8080/ \
  -H "Content-Type: application/toon" \
  -d $'toonrpc: "1.0"\nmethod: add\nparams[2]: 5,3\nid: 1'
```

Response:
```toon
toonrpc: "1.0"
result: 8
error: null
id: 1
```

## stdio Example

### Server

```bash
cargo run --bin calculator_stdio_server
```

Reads from stdin, writes to stdout.

### Client

```bash
# Send request via heredoc and pipe to client
printf 'toonrpc: "1.0"\nmethod: add\nparams[2]: 5,3\nid: 1\n\n' | \
  cargo run --bin calculator_stdio_client
```

Output (parsed message):
```
Single(
    Request(
        Request {
            toonrpc: "1.0",
            method: "add",
            params: ByPosition([Number(5), Number(3)]),
            id: Number(1),
        },
    ),
)
```

## Supported Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `add` | `add(a, b)` | Returns `a + b` |
| `subtract` | `subtract(a, b)` | Returns `a - b` |
| `multiply` | `multiply(a, b)` | Returns `a * b` |
| `divide` | `divide(a, b)` | Returns `a / b` (errors on `/0`) |

## Wire Format Example

```toon
# Request
toonrpc: "1.0"
method: add
params[2]: 5,3
id: 1

# Response (success)
toonrpc: "1.0"
result: 8
error: null
id: 1

# Response (error)
toonrpc: "1.0"
result: null
error: { code: -32602 message: "division by zero" data: null }
id: 1
```

## Message Framing

### HTTP
Each request/response is a complete HTTP message with `Content-Type: application/toon`.

### stdio
Messages are delimited by an empty line (`\n\n`). A complete TOON document is followed by an empty line to indicate the message boundary.
