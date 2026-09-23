//! The committed generated code is exactly what the generator produces from
//! the calculator IDL (the examples crate compiles and runs the Rust side, the
//! TypeScript package the other), and invalid IDL is refused with a reason.

use std::path::Path;

use reddb_io_toon_rpc_codegen::{generate_rust, generate_typescript, parse, IdlError};

fn repository_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn committed_code_matches_the_calculator_idl() {
    let service = parse(&repository_file(
        "crates/reddb-io-toon-rpc-codegen/examples/calculator.toonrpc",
    ))
    .unwrap();
    assert_eq!(
        generate_rust(&service),
        repository_file("crates/reddb-io-toon-rpc-examples/src/calculator_api.rs"),
        "regenerate with: reddb-io-toon-rpc-gen crates/reddb-io-toon-rpc-codegen/examples/calculator.toonrpc --rust"
    );
    assert_eq!(
        generate_typescript(&service),
        repository_file("packages/toon-rpc/test/generated/calculator.ts"),
        "regenerate with: reddb-io-toon-rpc-gen crates/reddb-io-toon-rpc-codegen/examples/calculator.toonrpc --ts"
    );
}

#[test]
fn output_is_deterministic_and_keeps_declaration_order() {
    let idl = "service: Ordered\ntypes:\n  Zeta:\n    b: i32\n    a: i32\n  Alpha:\n    z: bool\nmethods[2]:\n  - name: second\n    result: Zeta\n  - name: first\n    params:\n      y: Alpha\n      x: string[]?\n";
    let first = generate_rust(&parse(idl).unwrap());
    assert_eq!(first, generate_rust(&parse(idl).unwrap()));
    let order = ["struct Zeta", "struct Alpha", "fn second", "fn first"]
        .map(|needle| first.find(needle).unwrap());
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    assert!(first.contains("pub b: i32,\n    pub a: i32,"));
    assert!(first.contains("fn first(&self, y: Alpha, x: Option<Vec<String>>)"));
    let typescript = generate_typescript(&parse(idl).unwrap());
    assert!(typescript.contains("first(y: Alpha, x: string[] | null)"));
}

#[test]
fn invalid_idl_is_refused_with_a_reason() {
    for (idl, reason) in [
        ("methods[0]:", "`service` must name the service"),
        (
            "service: lower\nmethods[0]:",
            "must start with an uppercase letter",
        ),
        (
            "service: S\nmethods[1]:\n  - name: fn",
            "method `fn` is not a usable identifier",
        ),
        (
            "service: S\nmethods[1]:\n  - name: m\n    params:\n      a: Missing",
            "type `Missing` is not declared",
        ),
        (
            "service: S\nmethods[1]:\n  - name: m\n    params:\n      a: \"null\"",
            "`null` is only a result type",
        ),
        (
            "service: S\nevents[0]:\nmethods[0]:",
            "unknown IDL member `events`",
        ),
        (
            "service: S\nmethods[2]:\n  - name: m\n  - name: m",
            "method `m` is declared twice",
        ),
    ] {
        match parse(idl) {
            Err(IdlError::Invalid(message)) => {
                assert!(message.contains(reason), "{idl:?}: {message}")
            }
            other => panic!("{idl:?}: {other:?}"),
        }
    }
}
