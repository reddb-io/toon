#!/usr/bin/env bash
# Package (and so compile from the packaged sources) every RPC crate that the
# 0.31 line publishes. `cargo package` resolves the unpublished workspace
# dependencies through a local overlay, so the crates package together before
# any of them exists on crates.io; that overlay only takes publishable
# crates, so this runs once they leave `publish = false`. Extra arguments go
# to cargo (for example --allow-dirty after a version sync).
set -euo pipefail

# The codec packages with them: at a release commit every crate already
# carries the new version, which crates.io does not have yet, so the RPC
# crates can only resolve it from the same overlay.
PACKAGED_CRATES=(
  reddb-io-toon
  reddb-io-toon-rpc
  reddb-io-toon-rpc-stdio
  reddb-io-toon-rpc-tcp
  reddb-io-toon-rpc-http
  reddb-io-toon-rpc-ws
  reddb-io-toon-rpc-sse
  reddb-io-toon-rpc-mcp
  reddb-io-toon-rpc-acp
  reddb-io-toon-rpc-codegen
  reddb-io-toon-rpc-cli
)

args=()
for crate in "${PACKAGED_CRATES[@]}"; do
  args+=(-p "$crate")
done
cargo package "${args[@]}" "$@"
