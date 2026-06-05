#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
. ./scripts/demo-common.sh

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"
unset KAGI_API_TOKEN

build_demo_kagi

printf '\033c'
sleep 1.2
printf '$ kagi redirect list | jq -M '\''map({id, rule, enabled})[0:5]'\''\n'
sleep 0.4
"$KAGI_DEMO_BIN" redirect list | jq -M 'map({id, rule, enabled})[0:5]'
sleep 2
