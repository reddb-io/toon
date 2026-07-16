//! TOON (Token-Oriented Object Notation) parser and serializer.
//!
//! Implements the v3.3 working draft hosted at <https://github.com/toon-format/spec>.
//! The decoder honours the spec's decoder options (`indent`, `strict`, `expandPaths`);
//! the encoder emits the canonical default profile: comma document delimiter,
//! two-space indentation, no key folding.

include!("lib_parts/core.rs");
include!("lib_parts/toonl_and_cyclic_decode.rs");
include!("lib_parts/parser.rs");
include!("lib_parts/header_and_scalar.rs");
include!("lib_parts/encoder.rs");
include!("lib_parts/tabular_encoder.rs");
include!("lib_parts/tests.rs");
