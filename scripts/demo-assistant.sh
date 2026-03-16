#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"

printf '\033c'
printf '$ cargo run --quiet -- assistant "Reply with the word pear." | jq -M ...\n'
cargo run --quiet -- assistant "Reply with the word pear." \
  | jq -M '{
      thread_id: .thread.id,
      reply: .message.markdown,
      model: .message.profile.model_name
    }'
sleep 1
