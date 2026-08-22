#!/usr/bin/env bash
set -euo pipefail

# RPC crates run under `cargo test --workspace`; keep the established 95% line
# gate on the mature codec and CLI until each new transport has its own suite.
EXCLUDED_RPC_CRATES=(
  reddb-io-toon-rpc
  reddb-io-toon-rpc-stdio
  reddb-io-toon-rpc-codegen
  reddb-io-toon-rpc-cli
  reddb-io-toon-rpc-http
  reddb-io-toon-rpc-sse
  reddb-io-toon-rpc-tcp
  reddb-io-toon-rpc-ws
  reddb-io-toon-rpc-longpolling
  reddb-io-toon-rpc-examples
  reddb-io-toon-rpc-mcp
  reddb-io-toon-rpc-acp
)

EXCLUDE_ARGS=()
for crate in "${EXCLUDED_RPC_CRATES[@]}"; do
  EXCLUDE_ARGS+=(--exclude "$crate")
done

cargo llvm-cov --workspace "${EXCLUDE_ARGS[@]}" --fail-under-lines 95
