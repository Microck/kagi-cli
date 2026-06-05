#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
. ./scripts/demo-common.sh

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"

build_demo_kagi

printf '\033c'
sleep 1.2
printf '$ kagi ask-page https://rust-lang.org/ "What is this page about in one sentence?" | jq -M ...\n'
sleep 0.4
"$KAGI_DEMO_BIN" ask-page https://rust-lang.org/ "What is this page about in one sentence?" \
  | jq -M '{
      source: .source.url,
      thread_id: .thread.id,
      reply: .message.markdown
    }'
sleep 2
