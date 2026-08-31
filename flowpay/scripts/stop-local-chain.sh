#!/usr/bin/env bash
set -euo pipefail
for f in runtime/base-anvil.pid runtime/bsc-anvil.pid; do
  [[ -f "$f" ]] || continue
  kill "$(cat "$f")" 2>/dev/null || true
  rm -f "$f"
done
