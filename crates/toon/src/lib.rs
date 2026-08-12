//! TOON (Token-Oriented Object Notation) parser and serializer.
//!
//! TOON v4.1 is the only codec. The pre-v4 engine and its `Legacy*` API are
//! gone, and so is every name that told the two apart: the API is unsuffixed
//! because there is nothing else to distinguish it from.

include!("lib_parts/core.rs");
include!("lib_parts/toonl_and_cyclic_decode.rs");
include!("lib_parts/toonl_and_scalar.rs");
include!("lib_parts/stream.rs");
include!("lib_parts/stream_events.rs");
include!("lib_parts/stream_extensions.rs");
include!("lib_parts/stream_lines.rs");
include!("lib_parts/stream_value.rs");
include!("lib_parts/truncation.rs");
include!("lib_parts/cyclic_extension.rs");
include!("lib_parts/tabular_encoder.rs");
include!("lib_parts/encode.rs");
include!("lib_parts/api.rs");
include!("lib_parts/tests.rs");
