#!/usr/bin/env bash
# Assert every version declaration in the workspace agrees, so lockstep
# publishing (ADR 0003) can never ship a crate and a JS package that disagree.
#
# The declarations, and why each one is here:
#   Cargo.toml                      — the workspace version, the source of truth
#   crates/tq/Cargo.toml            — the tq → toon dependency requirement
#   Cargo.lock                      — resolved versions of the workspace crates
#   packages/toon/package.json      — the published npm package
#   packages/toon/src/version.js    — the version the JS package reports at runtime
#   packages/vscode-toon/package.json — the published VS Code extension
#   package.json                    — the workspace root, bumped by `pnpm version`
#
# Plus a drift guard against what is already published: a committed version
# behind the newest stable tag means a release went out that the tree never
# caught up with.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/workspace-members.sh
source "$(dirname "$0")/workspace-members.sh"

# Reads the first `version = "x"` of a Cargo manifest — the [workspace.package]
# one in the root, which every member inherits.
WORKSPACE_VERSION="$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)"
DEP_VERSION="$(sed -n 's|reddb-io-toon = { path = "../toon", version = "\([^"]*\)".*|\1|p' crates/tq/Cargo.toml)"
NPM_VERSION="$(sed -n 's|^  "version": "\([^"]*\)".*|\1|p' packages/toon/package.json)"
ROOT_VERSION="$(sed -n 's|^  "version": "\([^"]*\)".*|\1|p' package.json)"
JS_CONST_VERSION="$(sed -n "s|^export const VERSION = '\([^']*\)'.*|\1|p" packages/toon/src/version.js)"

if [[ -z "$WORKSPACE_VERSION" || "$WORKSPACE_VERSION" != "$DEP_VERSION" ]]; then
  echo "version drift: workspace=${WORKSPACE_VERSION:-<missing>} tq→toon dep=${DEP_VERSION:-<missing>}" >&2
  exit 1
fi
if [[ "$WORKSPACE_VERSION" != "$NPM_VERSION" ]]; then
  echo "version drift: workspace=${WORKSPACE_VERSION} @reddb-io/toon=${NPM_VERSION:-<missing>}" >&2
  exit 1
fi
if [[ "$WORKSPACE_VERSION" != "$ROOT_VERSION" ]]; then
  echo "version drift: workspace=${WORKSPACE_VERSION} root package.json=${ROOT_VERSION:-<missing>}" >&2
  exit 1
fi
if [[ "$WORKSPACE_VERSION" != "$JS_CONST_VERSION" ]]; then
  echo "version drift: workspace=${WORKSPACE_VERSION} toon/src/version.js=${JS_CONST_VERSION:-<missing>}" >&2
  exit 1
fi

# The VS Code extension carries the base x.y.z only (vsce rejects prerelease
# suffixes), so it is compared against the workspace version's base.
VSCODE_VERSION="$(sed -n 's|^  "version": "\([^"]*\)".*|\1|p' packages/vscode-toon/package.json)"
if [[ "${WORKSPACE_VERSION%%-*}" != "$VSCODE_VERSION" ]]; then
  echo "version drift: workspace=${WORKSPACE_VERSION} vscode-toon=${VSCODE_VERSION:-<missing>}" >&2
  exit 1
fi

# Cargo.lock pins the workspace crates by version too, and a stale lock entry
# is what makes `cargo publish` resolve a version the tree never declared.
# Members are read from the manifest so a new crate is covered on arrival.
lock_version_of() {
  awk -v want="$1" '
    /^name = / { gsub(/"/, "", $3); name = $3; next }
    /^version = / && name == want { gsub(/"/, "", $3); print $3; exit }
  ' Cargo.lock
}

while read -r crate_name; do
  [[ -n "$crate_name" ]] || continue
  locked="$(lock_version_of "$crate_name")"
  if [[ "$locked" != "$WORKSPACE_VERSION" ]]; then
    echo "version drift: workspace=${WORKSPACE_VERSION} Cargo.lock ${crate_name}=${locked:-<missing>}" >&2
    echo "hint: run scripts/sync-version.sh ${WORKSPACE_VERSION} (or \`cargo check\`) to refresh the lock" >&2
    exit 1
  fi
done < <(workspace_member_crate_names)

# Drift against what is already published. Prereleases (vX.Y.Z-next.N) are
# ignored: they are cut ahead of the stable line on purpose, so they say
# nothing about whether the tree is behind. A shallow clone or a tarball has
# no tags to compare against, which is a missing signal, not a failure.
LATEST_STABLE_TAG=""
if git rev-parse --git-dir >/dev/null 2>&1; then
  LATEST_STABLE_TAG="$(git tag --list 'v*' 2>/dev/null |
    grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' |
    sed 's|^v||' |
    sort -V |
    tail -1)" || true
fi

if [[ -z "$LATEST_STABLE_TAG" ]]; then
  echo "versions consistent: ${WORKSPACE_VERSION} (no stable tag visible — drift guard skipped)"
  exit 0
fi

# A prerelease is compared by its base: 0.10.2-next.3 is working toward 0.10.2.
BASE_VERSION="${WORKSPACE_VERSION%%-*}"
OLDEST="$(printf '%s\n%s\n' "$BASE_VERSION" "$LATEST_STABLE_TAG" | sort -V | head -1)"
if [[ "$BASE_VERSION" != "$LATEST_STABLE_TAG" && "$OLDEST" == "$BASE_VERSION" ]]; then
  echo "version drift: workspace=${WORKSPACE_VERSION} is behind published tag v${LATEST_STABLE_TAG}" >&2
  echo "hint: run scripts/sync-version.sh <version> to catch the tree up with the release" >&2
  exit 1
fi

echo "versions consistent: ${WORKSPACE_VERSION} (latest stable tag v${LATEST_STABLE_TAG})"
