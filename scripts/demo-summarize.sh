#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"

printf '\033c'
printf '$ cargo run --quiet -- summarize --subscriber --url https://www.rust-lang.org/ | jq -M ...\n'
cargo run --quiet -- summarize --subscriber --url https://www.rust-lang.org/ \
  | jq -M '{
      state: .data.state,
      prompt: .data.prompt,
      preview: (.data.markdown | split("\n\n")[0:2] | join("\n\n"))
    }'
sleep 1
