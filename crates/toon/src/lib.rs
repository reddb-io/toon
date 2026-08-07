//! TOON (Token-Oriented Object Notation) parser and serializer.
//!
//! Implements TOON v4.1 as the authoritative codec, with explicit `legacy`
//! methods for the former compatibility parser and serializer.

include!("lib_parts/core.rs");
include!("lib_parts/toonl_and_cyclic_decode.rs");
include!("lib_parts/parser.rs");
include!("lib_parts/header_and_scalar.rs");
include!("lib_parts/stream.rs");
include!("lib_parts/stream_lines.rs");
include!("lib_parts/stream_value.rs");
include!("lib_parts/decode_v4_extensions.rs");
include!("lib_parts/encoder.rs");
include!("lib_parts/tabular_encoder.rs");
include!("lib_parts/encode_v4.rs");
include!("lib_parts/api_v4.rs");
include!("lib_parts/tests.rs");
