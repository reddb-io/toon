#!/usr/bin/env bash
# Sync every version declaration in the workspace to the given version
# (reddb-style, ADR 0003). Both crates inherit [workspace.package] version;
# the tq → toon dependency requirement is the only other declaration.
set -euo pipefail
VERSION="${1:?usage: sync-version.sh <version>}"

sed -i "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
sed -i "s|\(reddb-io-toon = { path = \"../toon\", version = \"\)[^\"]*|\1${VERSION}|" crates/tq/Cargo.toml

bash "$(dirname "$0")/check-versions.sh"
