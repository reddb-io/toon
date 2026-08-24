#!/usr/bin/env bash
set -euo pipefail

# Runs the given command with CARGO_TARGET_DIR moved to a shared real-disk
# cache when this checkout sits on tmpfs. Worker worktrees can live on a
# small tmpfs (4 GiB /tmp), where a workspace target tree does not fit and
# the build dies mid-link (ld SIGBUS) or mid-compile (ENOSPC). On a real
# disk the default in-worktree target is kept, so local runs and CI are
# unchanged. An explicit CARGO_TARGET_DIR always wins.
if [ -z "${CARGO_TARGET_DIR:-}" ] && [ "$(stat -f -c %T . 2>/dev/null)" = "tmpfs" ]; then
  export CARGO_TARGET_DIR="${HOME}/.cache/toon-test-target"
  mkdir -p "$CARGO_TARGET_DIR"
fi

exec "$@"
