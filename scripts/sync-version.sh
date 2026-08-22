#!/usr/bin/env bash
# Sync every version declaration in the workspace to the given version
# (reddb-style, ADR 0003). Both crates inherit [workspace.package] version;
# the tq → toon dependency requirement, the resolved Cargo.lock entries, the
# npm package @reddb-io/toon and the VERSION constant it reports at runtime,
# the generated JavaScript artifact, the VS Code extension, and the workspace
# root package.json are the other declarations, so a release ships every
# artefact at the same version.
#
# Wired into the root package.json `version` lifecycle hook: `pnpm version
# patch` bumps the root manifest, this script fans the new version out to the
# rest, and the hook stages them into pnpm's version commit.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/workspace-members.sh
source "$(dirname "$0")/workspace-members.sh"
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
sed_i "s|^  \"version\": \".*\"|  \"version\": \"${VERSION}\"|" package.json
# The runtime constant the JS package reports; a source constant on purpose,
# so nothing has to read package.json at runtime. Keep the committed generated
# JavaScript in lockstep with its TypeScript source for consumers of dist/.
sed_i "s|^export const VERSION = '.*'|export const VERSION = '${VERSION}'|" packages/toon/src/version.ts
sed_i "s|^export const VERSION = '.*'|export const VERSION = '${VERSION}'|" packages/toon/dist/version.js

# Cargo.lock pins the workspace crates by version as well, and a stale entry is
# what lets a publish resolve a version the tree never declared. Rewriting the
# stanzas here keeps the sync toolchain-free: no cargo invocation, no registry
# access. Path members carry no `source` key, so name + version is the whole
# edit. Members are read from the manifest so a new crate is covered on arrival.
rewrite_lock_version() {
  local crate="$1"
  awk -v want="$crate" -v version="$VERSION" '
    /^name = / { split($0, field, "\""); name = field[2] }
    /^version = / && name == want && !rewritten {
      print "version = \"" version "\""
      rewritten = 1
      next
    }
    { print }
  ' Cargo.lock >Cargo.lock.sync-tmp
  mv Cargo.lock.sync-tmp Cargo.lock
}

while read -r crate_name; do
  [[ -n "$crate_name" ]] || continue
  rewrite_lock_version "$crate_name"
done < <(workspace_member_crate_names)

# The toon-rpc npm packages ship in lockstep with the workspace. They were
# bumped by hand for 0.29.x and drifted (0.29.0 / 0.29.2 / 0.29.3 across three
# manifests in one tree), which is exactly the drift this script exists to make
# impossible. Covered here and asserted in check-versions.sh.
for rpc_pkg in packages/toon-rpc packages/toon-rpc-mcp packages/toon-rpc-acp; do
  sed_i "s|^  \"version\": \".*\"|  \"version\": \"${VERSION}\"|" "${rpc_pkg}/package.json"
done

# The VS Code extension ships in lockstep too, but vsce and the Marketplace
# only accept plain x.y.z versions, so prerelease suffixes (-next.N) are
# stripped down to the base version.
EXTENSION_VERSION="${VERSION%%-*}"
sed_i "s|^  \"version\": \".*\"|  \"version\": \"${EXTENSION_VERSION}\"|" packages/vscode-toon/package.json

bash "$(dirname "$0")/check-versions.sh"
