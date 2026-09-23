//! Code generation for TOON-RPC services: a `.toonrpc` IDL in, Rust and
//! TypeScript out. The generated code compiles as is and serves or calls the
//! service through `reddb-io-toon-rpc` and `@reddb-io/toon-rpc`.

mod idl;
mod rust;
mod typescript;

pub use idl::{parse, Field, IdlError, Method, Service, Type, TypeDef};

/// The Rust module for `service`: types, trait, `register_*` and a client.
pub fn generate_rust(service: &Service) -> String {
    rust::generate(service)
}

/// The TypeScript module for `service`: types, interface, `register*` and a client.
pub fn generate_typescript(service: &Service) -> String {
    typescript::generate(service)
}
