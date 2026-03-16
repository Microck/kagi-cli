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
printf '$ kagi assistant "plan a private obsidian workflow for cafe work. give me 3 setup tips and a short checklist." | jq -M ...\n'
sleep 0.4
kagi assistant "plan a private obsidian workflow for cafe work. give me 3 setup tips and a short checklist." \
  | jq -M '{
      thread_id: .thread.id,
      reply: .message.markdown,
      model: .message.profile.model_name
    }'
sleep 2
