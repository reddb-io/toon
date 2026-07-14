#!/usr/bin/env bash
# Assert the workspace version and the tq → toon dependency requirement agree,
# so lockstep publishing (ADR 0003) can never ship crates that disagree.
set -euo pipefail
cd "$(dirname "$0")/.."

WORKSPACE_VERSION="$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)"
DEP_VERSION="$(sed -n 's|reddb-io-toon = { path = "../toon", version = "\([^"]*\)".*|\1|p' crates/tq/Cargo.toml)"

if [[ -z "$WORKSPACE_VERSION" || "$WORKSPACE_VERSION" != "$DEP_VERSION" ]]; then
  echo "version drift: workspace=${WORKSPACE_VERSION:-<missing>} tq→toon dep=${DEP_VERSION:-<missing>}" >&2
  exit 1
fi
echo "versions consistent: ${WORKSPACE_VERSION}"
