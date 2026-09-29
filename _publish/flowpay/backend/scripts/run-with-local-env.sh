#!/bin/sh
set -eu

if [ "${FLOWPAY_WAIT_FOR_LOCAL_ENV:-1}" = "1" ]; then
  i=0
  while [ ! -s /runtime/local.env ]; do
    i=$((i + 1))
    if [ "$i" -gt 120 ]; then
      echo "timed out waiting for /runtime/local.env" >&2
      exit 1
    fi
    sleep 1
  done
  set -a
  . /runtime/local.env
  set +a
fi

exec "$@"
