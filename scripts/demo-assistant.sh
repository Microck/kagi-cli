#!/usr/bin/env bash
#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"

cargo build --quiet
mkdir -p /tmp/kagi-demo-bin
ln -sf "$PWD/target/debug/kagi" /tmp/kagi-demo-bin/kagi
export PATH="/tmp/kagi-demo-bin:$PATH"

THREAD_ID=""
cleanup() {
  if [[ -n "$THREAD_ID" ]]; then
    kagi assistant thread delete "$THREAD_ID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

PROMPT="plan a calm terminal research session in 3 bullets."
FOLLOWUP="turn that into a 3-item checklist."

printf '\033c'
sleep 1.2

printf '$ RESPONSE=$(kagi assistant --model gpt-5-mini "%s")\n' "$PROMPT"
sleep 0.4
RESPONSE=$(kagi assistant --model gpt-5-mini "$PROMPT")
THREAD_ID=$(printf '%s' "$RESPONSE" | jq -r '.thread.id')
printf '%s\n' "$RESPONSE" | jq -M '{
  thread_id: .thread.id,
  reply: .message.markdown,
  model: .message.profile.model_name
}'
sleep 1.8

printf '$ kagi assistant --thread-id "$THREAD_ID" "%s" | jq -M ...\n' "$FOLLOWUP"
sleep 0.4
kagi assistant --thread-id "$THREAD_ID" "$FOLLOWUP" | jq -M '{
  thread_id: .thread.id,
  reply: .message.markdown
}'
sleep 1.8

printf '$ kagi assistant thread export "$THREAD_ID" | sed -n '\''1,14p'\''\n'
sleep 0.4
kagi assistant thread export "$THREAD_ID" | sed -n '1,14p'
sleep 2
