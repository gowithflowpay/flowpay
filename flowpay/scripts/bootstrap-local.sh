#!/usr/bin/env bash
set -euo pipefail
cp -n .env.example .env 2>/dev/null || true
./scripts/reset-local.sh
printf '%s\n' \
  'FlowPay local state is ready. Start services with make dev.' \
  'Merchant: http://localhost:3000' \
  'Checkout: http://localhost:3001' \
  'API:      http://localhost:8080'
