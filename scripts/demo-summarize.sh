#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
. ./scripts/demo-common.sh

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"

build_demo_kagi

printf '\033c'
sleep 1.2
printf '$ kagi summarize --subscriber --url https://mullvad.net/en/browser | jq -M ...\n'
sleep 0.4
"$KAGI_DEMO_BIN" summarize --subscriber --url https://mullvad.net/en/browser \
  | jq -M '{
      state: .data.state,
      prompt: .data.prompt,
      preview: (.data.markdown | split("\n\n")[0:2] | join("\n\n"))
    }'
sleep 2
