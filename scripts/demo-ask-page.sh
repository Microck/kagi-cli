#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"

cargo build --quiet
mkdir -p /tmp/kagi-demo-bin
ln -sf "$PWD/target/debug/kagi" /tmp/kagi-demo-bin/kagi
export PATH="/tmp/kagi-demo-bin:$PATH"

printf '\033c'
sleep 1.2
printf '$ kagi ask-page https://rust-lang.org/ "What is this page about in one sentence?" | jq -M ...\n'
sleep 0.4
kagi ask-page https://rust-lang.org/ "What is this page about in one sentence?" \
  | jq -M '{
      source: .source.url,
      thread_id: .thread.id,
      reply: .message.markdown
    }'
sleep 2
