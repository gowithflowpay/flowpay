#!/usr/bin/env bash
set -euo pipefail
exec ./scripts/run-e2e-evals.sh "${1:-all}"
