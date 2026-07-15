#!/usr/bin/env bash
# Confirm a registry actually serves an exact version before the release job
# calls the publish a success (issue #59: the v0.1.0 cut read as a ghost
# publish because the npm registry 404'd the fresh version for minutes).
# Bounded retry: propagation delay passes, real absence fails the job.
set -euo pipefail
KIND="${1:?usage: verify-published.sh <npm|crates> <name> <version>}"
NAME="${2:?usage: verify-published.sh <npm|crates> <name> <version>}"
VERSION="${3:?usage: verify-published.sh <npm|crates> <name> <version>}"
ATTEMPTS="${VERIFY_ATTEMPTS:-30}"
DELAY_SECONDS="${VERIFY_DELAY_SECONDS:-10}"

check() {
  case "$KIND" in
    npm)
      npm view "${NAME}@${VERSION}" version >/dev/null 2>&1
      ;;
    crates)
      # The sparse index is what cargo itself polls for publish visibility;
      # it serves one JSON line per published version.
      local prefix
      case "${#NAME}" in
        1) prefix="1" ;;
        2) prefix="2" ;;
        3) prefix="3/${NAME:0:1}" ;;
        *) prefix="${NAME:0:2}/${NAME:2:2}" ;;
      esac
      curl -fsS --max-time 20 -A "reddb-io/toon release workflow" \
        "https://index.crates.io/${prefix}/${NAME}" 2>/dev/null \
        | grep -q "\"vers\":\"${VERSION}\""
      ;;
    *)
      echo "unknown registry kind: ${KIND}" >&2
      exit 2
      ;;
  esac
}

for attempt in $(seq 1 "$ATTEMPTS"); do
  if check; then
    echo "verified: ${NAME}@${VERSION} is served by the ${KIND} registry (attempt ${attempt}/${ATTEMPTS})"
    exit 0
  fi
  if ((attempt < ATTEMPTS)); then
    echo "  ${NAME}@${VERSION} not visible on ${KIND} yet (attempt ${attempt}/${ATTEMPTS}); retrying in ${DELAY_SECONDS}s"
    sleep "$DELAY_SECONDS"
  fi
done

echo "FAIL: ${NAME}@${VERSION} never appeared on the ${KIND} registry after ${ATTEMPTS} attempts" >&2
exit 1
