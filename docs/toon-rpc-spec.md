# TOON-RPC 1.0 Core Protocol

**Wire version:** 1.0
**Status:** Normative recovery contract
**TOON checkpoint:** v4.1.1, `toon-format/spec` revision
`62f16b369408180f1faf1cba7da1b46d1f336f12`

TOON-RPC is an RPC envelope protocol encoded as UTF-8 TOON. It borrows the
request, notification, response, error, and batch model from JSON-RPC 2.0, but
it is a separate wire protocol. A TOON-RPC peer MUST NOT advertise a TOON-RPC
document as JSON-RPC, MCP, or ACP traffic.

The package recovery target is 0.30.0. Package versions are not wire versions;
every message defined here carries `toonrpc: "1.0"`.

## 1. Normative Language

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, NOT RECOMMENDED, MAY, and OPTIONAL are to be interpreted as
described in BCP 14 when they appear in capitals.

## 2. Data Model

A TOON-RPC document MUST be valid UTF-8 and valid TOON at the pinned checkpoint.
Its values are the common JSON-compatible model:

- null;
- booleans;
- finite numbers representable as IEEE-754 binary64 values;
- UTF-8 strings;
- arrays; and
- objects with string keys.

NaN, positive or negative infinity, binary values, and implementation-specific
integer types are outside the core wire model. Every integer value, not only an
ID, MUST be within the JSON-safe range. Decimal fractions are decoded using
correct IEEE-754 binary64 rounding. A numeric token that overflows to a
non-finite value or an integer token outside the safe range MUST be rejected;
it MUST NOT be rounded to another integer or reinterpreted as a string.
Applications that require larger exact integers SHOULD encode them as strings.

Object member order has no meaning. Unless this document says otherwise, a
recipient MUST ignore unknown object members. Unknown members do not relax the
requirements on known members.

## 3. IDs

An ID is one of:

- a string;
- null; or
- an integer from `-9007199254740991` through `9007199254740991`, inclusive.

Fractional numbers, unsafe integers, booleans, arrays, and objects are invalid
IDs. Implementations MUST preserve the ID value and type exactly.

Only the absence of the `id` member denotes a notification. An explicit
`id: null` is a request and MUST receive a response. A malformed object does not
become a notification merely because it has no `id` member.

## 4. Request Objects

A Request Object MUST contain:

| Member | Requirement |
|---|---|
| `toonrpc` | REQUIRED string with the exact value `"1.0"` |
| `method` | REQUIRED non-empty string; names beginning with `rpc.` are reserved |
| `params` | OPTIONAL array for positional parameters or object for named parameters |
| `id` | OPTIONAL ID; absence makes the request a notification |

`params: null`, scalar `params`, and any other `params` shape are invalid
request envelopes. Absence is preserved as absence; a protocol implementation
MUST NOT silently rewrite it to an empty array or object.

Request with positional parameters:

```toon
toonrpc: "1.0"
method: "subtract"
params[2]: 42,23
id: 1
```

Notification with named parameters:

```toon
toonrpc: "1.0"
method: "cache.invalidate"
params:
  key: "users"
```

A server MUST invoke a handler only after the complete Request Object has passed
envelope validation. It MUST invoke a valid notification exactly once and MUST
NOT produce an RPC Response Object for it.

## 5. Response Objects

A Response Object has these protocol-defined members:

| Member | Requirement |
|---|---|
| `toonrpc` | REQUIRED string with the exact value `"1.0"` |
| `id` | REQUIRED ID copied from a valid request, or null for an uncorrelated protocol error |
| `result` | REQUIRED on success and forbidden on error; any core data-model value is valid |
| `error` | REQUIRED on error and forbidden on success; MUST be an Error Object |

Exactly one of `result` and `error` MUST be present. Member presence, not value,
selects the branch: `result: null` is a valid success. A response with both
members or neither member is invalid.

A conforming emitter MUST generate no response members beyond `toonrpc`, `id`,
and the selected branch. A recipient still follows the general compatibility
rule and ignores unknown members after validating every known member.

Success:

```toon
toonrpc: "1.0"
result: 19
id: 1
```

Error:

```toon
toonrpc: "1.0"
error:
  code: -32601
  message: "Method not found"
id: 1
```

A client MUST validate the version, ID, branch exclusivity, and Error Object
before accepting a response. It MUST reject a response that cannot be
correlated to a pending request.

### 5.1 Client Batch Processing

A response batch is a non-empty root array of Response Objects. A client MUST
validate and correlate every entry independently:

1. A valid entry settles its matching pending call at most once.
2. A malformed entry is rejected without preventing valid sibling entries from
   settling their calls.
3. An unknown ID is rejected without affecting sibling entries.
4. If an ID appears more than once, the first valid entry settles the call and
   every later entry with that ID is rejected as a duplicate.
5. Calls without a valid matching entry remain pending.
6. An empty response batch is invalid.

Implementations MUST surface rejected entries through their normal protocol
error or diagnostic mechanism; they MUST NOT silently settle a call from an
invalid entry.

## 6. Error Objects

An Error Object contains:

| Member | Requirement |
|---|---|
| `code` | REQUIRED signed 32-bit integer |
| `message` | REQUIRED string with a concise description |
| `data` | OPTIONAL core data-model value with implementation-specific detail |

The presence of `data` is significant. `data: null` differs from an absent
`data` member and MUST survive a decode/encode round trip.

### 6.1 Reserved Codes

| Code | Meaning | Use |
|---|---|---|
| -32700 | Parse error | Input is not valid UTF-8 or valid TOON |
| -32600 | Invalid Request | Decoded value is not a valid request envelope |
| -32601 | Method not found | No handler exists for a valid request method |
| -32602 | Invalid params | A valid request's structured parameters fail method validation |
| -32603 | Internal error | Unexpected internal RPC failure |
| -32099 through -32000 | Server error | Implementation-defined server failures |

Codes from `-32768` through `-32000` are reserved for protocol and server
errors. Applications MAY use any other signed 32-bit code. Implementations MUST
preserve unknown application codes exactly rather than mapping them to a closed
enum or a different server-error code.

### 6.2 Error Correlation

- Parse errors use `id: null` because no envelope was decoded.
- Invalid Request errors use `id: null`, even if the malformed value contains
  something named `id`.
- Method-not-found, invalid-params, handler, and internal errors for a valid
  request preserve that request's ID.
- An error caused by a valid notification produces no RPC response document.

Implementations MAY choose diagnostic wording, except where a conformance case
explicitly pins `message`. Conformance always pins the error code, ID, and
response shape.

## 7. Batch Documents

A batch request is a non-empty root array. Each array entry is processed and
validated independently as a Request Object.

```toon
[3]:
  - toonrpc: "1.0"
    method: "sum"
    params[3]: 1,2,4
    id: "1"
  - toonrpc: "1.0"
    method: "notify.hello"
    params[1]: 7
  - toonrpc: "1.0"
    method: "subtract"
    params[2]: 42,23
    id: "2"
```

Batch processing follows these rules:

1. Every malformed entry contributes an Invalid Request response with
   `id: null`; it does not prevent valid sibling entries from running.
2. Valid notification entries run exactly once and contribute no response
   element.
3. The response to a batch is an array, even when only one response element
   remains after notifications are omitted.
4. Response element order is not guaranteed.
5. An all-notification batch produces no RPC response document.
6. An empty root array is invalid and produces one non-batch Invalid Request
   Response Object with `id: null`.
7. A syntax error prevents decoding the root and produces one non-batch Parse
   Error Response Object with `id: null`.

A server MAY process entries concurrently if handler and transport guarantees
permit it.

## 8. Processing Boundary

The core protocol operates on complete RPC documents. It does not define byte
framing, connection ownership, retries, cancellation, backpressure, HTTP
status codes, or connection shutdown. A client or transport profile MUST define
those lifecycle rules without weakening response validation or correlation.

Transport profiles MUST preserve the document boundaries and no-response
semantics defined here:

- request/response transports such as HTTP map one request body to zero or one
  response body; the response is scoped to that exchange and cannot settle a
  different concurrent call; HTTP is not a fake duplex stream;
- frame transports such as WebSocket carry one complete document per selected
  frame profile; and
- byte streams such as TCP, Unix sockets, and stdio require an explicit framing
  profile and MUST NOT infer boundaries from arbitrary newlines.

When a direct request/response exchange completes without a valid response for
its initiating call, the exchange is exhausted and that call terminates with a
protocol error. The rule in section 5.1 that unmatched calls remain pending
continues to apply to duplex response documents, where later documents can still
provide the match. A duplex transport's close operation MUST terminate its
receive iterator; cancellation-aware operations SHOULD stop promptly when their
supplied abort signal fires.

For example, an HTTP profile will normally represent a notification-only result
with status 204 and no RPC body. A stream profile emits no response frame. These
are transport rules, not extra RPC messages.

## 9. Deferred Features

SSE, long polling, subscriptions, cancellation, capability negotiation, IDL,
code generation, and protocol multiplexing are not part of this core contract.
They require separate profiles that cannot weaken the core envelope rules.

MCP and legacy ACP adapters must target their independently pinned standards.
TOON-RPC extensions MUST NOT be presented as standard MCP or ACP wire formats.

## 10. Conformance Corpus

The normative machine-readable vectors live at:

- `tests/corpus/toon-rpc/fixtures.schema.json`; and
- `tests/corpus/toon-rpc/contract.json`.

The corpus defines declarative fixture handlers so Rust and TypeScript runners
exercise the same service behavior. A case provides exactly one source form:
exact wire text, a decoded logical value, raw bytes encoded as canonical base64,
or the explicit pair of `wire` and `value`. For the paired form, a runner MUST
first verify that decoding `wire` produces `value`.

Client inputs declare `pendingIds`; runners MUST seed exactly those pending
calls before delivering the response. Client batch expectations identify every
settled call, rejected entry, and still-pending ID. TypeScript seeds the
production `Client`; Rust uses its harness oracle until slice 8 recovers the
production Rust client. Case names are globally
unique fixture identifiers, and runners MUST reject duplicate names across the
complete `valid` and `malformed` arrays before executing any case.

When an expectation contains `calls`, it is the exact per-method invocation
map: omitted methods have count zero and the sum of its values MUST equal
`callCount`. Before executing a `bytesBase64` case, a runner MUST decode and
re-encode the bytes and reject the fixture unless the result exactly matches
the source string; this enforces canonical base64 beyond JSON Schema's lexical
check.

Server cases pin handler call counts and one of: no response, success, error, or
batch response. Client cases pin acceptance or rejection of response envelopes.
Success and error matchers require exact protocol members; extra members are
permitted on received requests but MUST NOT appear in generated responses.

Batch responses are always compared without relying on order. Object member
ordering is also ignored. Error wording is compared only when `message` is
present in the expectation.

The corpus is the acceptance contract for the 0.30 recovery. Rust and
TypeScript semantic runners execute every vector directly from these shared
files; expected failure ledgers are not permitted for this corpus. Server cases
exercise the production dispatcher/server. TypeScript client cases exercise the
production client and its public diagnostic mechanism; Rust client cases remain
harness-only until slice 8.

## 11. Implementation Status

The TypeScript client owns one receive pump for a framed duplex transport and
supports a separate direct request/response contract. Pending calls are removed
before settlement on success, RPC error, abort, timeout, send failure, transport
failure/completion, or client close. Invalid, unknown-ID, and duplicate-ID
responses are observable diagnostics, and valid batch siblings are isolated.

The existing TypeScript and Rust packages remain quarantined while Spec #389 is
in progress. The concrete TypeScript transports and Rust production client are
still deferred, so shared semantic coverage does not imply that every production
component conforms. Publication resumes only after lifecycle, transport,
package, and exact-commit release gates pass.
