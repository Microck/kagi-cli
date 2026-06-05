#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
. ./scripts/demo-common.sh

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"
unset KAGI_API_TOKEN

build_demo_kagi

printf '\033c'
sleep 1.2
printf '$ kagi search --format pretty --region us --time year --order recency "rust release notes"\n'
sleep 0.4
"$KAGI_DEMO_BIN" search --format pretty --region us --time year --order recency "rust release notes" | sed -n '1,12p'
sleep 2
