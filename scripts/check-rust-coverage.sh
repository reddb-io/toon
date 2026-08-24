#!/usr/bin/env bash
set -euo pipefail

# The spec-conformance tests need the vendored fixture submodules; validation
# moments may run this script in a fresh worktree where project setup has not,
# so the gate provisions its own inputs instead of assuming them.
git -C "$(dirname "$0")/.." submodule update --init vendor/toon vendor/toon-spec

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

# Coverage builds write multi-GB target trees; worker worktrees can live on a
# small tmpfs (4 GiB /tmp), where the build dies mid-compile on ENOSPC. Build
# into a shared cache on the real disk instead — cargo locks the directory
# against concurrent builds, and reuse keeps the gate warm across runs.
# Override with TOON_COVERAGE_TARGET_DIR when needed.
export CARGO_TARGET_DIR="${TOON_COVERAGE_TARGET_DIR:-${HOME}/.cache/toon-coverage-target}"
mkdir -p "$CARGO_TARGET_DIR"

run_coverage() {
  cargo llvm-cov --workspace "${EXCLUDE_ARGS[@]}" --fail-under-lines 95
}

# A run killed mid-build (worker timeout, operator interrupt) can leave the
# shared cache with profdata pointing at half-written test binaries, which
# fails every later run. Clean only the coverage artifacts and retry once
# before reporting red.
if ! run_coverage; then
  cargo llvm-cov clean --workspace
  run_coverage
fi
