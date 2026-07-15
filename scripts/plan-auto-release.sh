#!/usr/bin/env bash
# Decide whether the commits on main since the last release warrant a new
# stable release, and which version it should be (issue #60). Conventional
# commit mapping:
#
#   breaking (`type!:` subject or `BREAKING CHANGE` in the body)
#          → major from 1.0, minor while pre-1.0
#   feat:  → minor
#   fix: / perf: / refactor: / revert:  → patch
#   everything else (docs, chore, test, ci, style, build, merges,
#   non-conventional)  → no bump
#
# The release baseline is the most recent of the last stable tag and the last
# `chore: release X.Y.Z` commit. The chore commit matters because the stable
# tag is only created minutes later by the dispatched release run — without
# it, back-to-back merges would double-release.
#
# Emits `bump=<none|patch|minor|major>` and `version=<X.Y.Z>` to
# $GITHUB_OUTPUT when set; always prints them to stdout for local runs.
set -euo pipefail

LAST_TAG="$(git tag --merged HEAD --list 'v*' \
  | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' \
  | sort -V | tail -n1 || true)"

BASE_REF="${LAST_TAG}"
BASE_VERSION="${LAST_TAG#v}"

SYNC_LINE="$(git log --first-parent --format='%H %s' ${LAST_TAG:+${LAST_TAG}..}HEAD \
  | grep -E ' chore: release [0-9]+\.[0-9]+\.[0-9]+$' \
  | head -n1 || true)"
if [[ -n "$SYNC_LINE" ]]; then
  BASE_REF="${SYNC_LINE%% *}"
  BASE_VERSION="${SYNC_LINE##* }"
fi

emit() {
  local bump="$1" version="$2"
  echo "baseline: ${BASE_REF:-<none>} (${BASE_VERSION:-<none>})"
  echo "bump=${bump}"
  echo "version=${version}"
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    {
      echo "bump=${bump}"
      echo "version=${version}"
    } >> "$GITHUB_OUTPUT"
  fi
}

if [[ -z "$BASE_REF" ]]; then
  echo "skip: no stable tag or 'chore: release' commit to baseline from — cut the first release manually"
  emit none ""
  exit 0
fi

RANGE="${BASE_REF}..HEAD"
SUBJECTS="$(git log --format='%s' "$RANGE")"
BODIES="$(git log --format='%b' "$RANGE")"

if [[ -z "$SUBJECTS" ]]; then
  echo "skip: no commits since ${BASE_REF}"
  emit none ""
  exit 0
fi

has_breaking=false
has_minor=false
has_patch=false
grep -qE '^[a-z]+(\([^)]+\))?!:' <<<"$SUBJECTS" && has_breaking=true
grep -q 'BREAKING CHANGE' <<<"$BODIES" && has_breaking=true
grep -qE '^feat(\([^)]+\))?:' <<<"$SUBJECTS" && has_minor=true
grep -qE '^(fix|perf|refactor|revert)(\([^)]+\))?:' <<<"$SUBJECTS" && has_patch=true

IFS=. read -r MAJOR MINOR PATCH <<<"$BASE_VERSION"

BUMP=none
if $has_breaking; then
  # Pre-1.0 the public contract is still fluid: breaking changes ride the
  # minor bump, the conventional 0.x semver reading.
  if ((MAJOR >= 1)); then BUMP=major; else BUMP=minor; fi
elif $has_minor; then
  BUMP=minor
elif $has_patch; then
  BUMP=patch
fi

case "$BUMP" in
  none)
    echo "skip: no releasable commits (feat/fix/perf/refactor/revert/breaking) in ${RANGE}"
    emit none ""
    ;;
  major) emit major "$((MAJOR + 1)).0.0" ;;
  minor) emit minor "${MAJOR}.$((MINOR + 1)).0" ;;
  patch) emit patch "${MAJOR}.${MINOR}.$((PATCH + 1))" ;;
esac
