#!/usr/bin/env bash
set -euo pipefail

limit=1200
failed=0

while IFS= read -r path; do
  case "$path" in
    vendor/*|node_modules/*|dist/*|target/*|.red/tmp/*|benchmarks/datasets/*)
      continue
      ;;
    *.rs|*.js|*.mjs|*.ts|*.tsx)
      ;;
    *)
      continue
      ;;
  esac

  lines=$(wc -l < "$path")
  if [ "$lines" -gt "$limit" ]; then
    printf '%s: %s lines exceeds %s\n' "$path" "$lines" "$limit" >&2
    failed=1
  fi
done < <(git ls-files)

exit "$failed"
