#!/usr/bin/env bash
# Sync every version declaration in the workspace to the given version
# (reddb-style, ADR 0003). Both crates inherit [workspace.package] version;
# the tq → toon dependency requirement and the npm package @reddb-io/toon are
# the other declarations, so a release ships the crates and the JS package at
# the same version.
set -euo pipefail
VERSION="${1:?usage: sync-version.sh <version>}"

# GNU sed edits in place with a bare -i; BSD sed (macOS runners) requires an
# explicit suffix argument. Feature-detect instead of matching on uname.
sed_i() {
  if sed --version >/dev/null 2>&1; then
    sed -i "$@"
  else
    sed -i '' "$@"
  fi
}

sed_i "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
sed_i "s|\(reddb-io-toon = { path = \"../toon\", version = \"\)[^\"]*|\1${VERSION}|" crates/tq/Cargo.toml
# Only the package's own "version" key, which is the second line of the file —
# anchoring on the two-space indent keeps this away from any nested version.
sed_i "s|^  \"version\": \".*\"|  \"version\": \"${VERSION}\"|" packages/toon/package.json

bash "$(dirname "$0")/check-versions.sh"
