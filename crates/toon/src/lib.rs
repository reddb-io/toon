//! TOON (Token-Oriented Object Notation) parser and serializer.
//!
//! Implements TOON v4.1 as the one codec. The public API is unsuffixed because
//! there is nothing else to distinguish it from; the surviving `legacy` methods
//! are the pre-v4.1 parser and serializer, kept only until they are deleted.

include!("lib_parts/core.rs");
include!("lib_parts/toonl_and_cyclic_decode.rs");
include!("lib_parts/parser.rs");
include!("lib_parts/header_and_scalar.rs");
include!("lib_parts/stream.rs");
include!("lib_parts/stream_events.rs");
include!("lib_parts/stream_extensions.rs");
include!("lib_parts/stream_lines.rs");
include!("lib_parts/stream_value.rs");
include!("lib_parts/truncation.rs");
include!("lib_parts/encoder.rs");
include!("lib_parts/tabular_encoder.rs");
include!("lib_parts/encode.rs");
include!("lib_parts/api.rs");
include!("lib_parts/tests.rs");
