# reddb-io-toon-rpc-codegen

Generates Rust and TypeScript code for a TOON-RPC service from a `.toonrpc`
IDL. The output compiles as is and serves or calls the service through
`reddb-io-toon-rpc` and `@reddb-io/toon-rpc`.

```bash
reddb-io-toon-rpc-gen calculator.toonrpc --rust > src/calculator_api.rs
reddb-io-toon-rpc-gen calculator.toonrpc --ts > src/calculator.ts
# or, through the CLI:
reddb-io-toon-rpc generate calculator.toonrpc --lang ts
```

## IDL

A `.toonrpc` file is a TOON document ([`examples/calculator.toonrpc`](examples/calculator.toonrpc)):

```toon
service: Calculator
version: "1.0"
types:
  Vec2:
    x: f64
    y: f64
methods[2]:
  - name: add
    params:
      a: f64
      b: f64
    result: f64
  - name: norm
    params:
      v: Vec2
    result: f64
```

Types are `bool`, `i32`, `i64`, `u32`, `u64`, `f64`, `string`, `json` (any
value), `null` (results only), a type declared under `types`, `T[]` for a
list and `T?` for an optional value. Names must be identifiers that are not
keywords in either language. Output follows declaration order, so it is
deterministic.

## Output

For a service `Calculator`, each language gets:

- the declared types (Rust structs with serde derives, TypeScript interfaces);
- the service trait (Rust) or interface (TypeScript);
- `register_calculator(&mut Dispatcher, Arc<dyn Calculator>)` /
  `registerCalculator(server, service)`, which accept params by name or by
  position (in declaration order) and refuse anything else with Invalid params;
- `CalculatorClient`, a typed client over any `Client`.

The Rust module needs `serde` (with `derive`) and `serde_json` next to
`reddb-io-toon-rpc`, and carries inner attributes, so use it as its own module
file. The calculator's generated code is committed in
`crates/reddb-io-toon-rpc-examples/src/calculator_api.rs` and
`packages/toon-rpc/test/generated/calculator.ts`, where it is compiled and
exercised; the codegen tests fail if either drifts from the IDL.
