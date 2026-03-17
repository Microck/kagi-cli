#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"
unset KAGI_API_TOKEN

cargo build --quiet
mkdir -p /tmp/kagi-demo-bin
ln -sf "$PWD/target/debug/kagi" /tmp/kagi-demo-bin/kagi
export PATH="/tmp/kagi-demo-bin:$PATH"

printf '\033c'
sleep 1.2
printf '$ kagi search --format pretty "obsidian cli daily notes workflow"\n'
sleep 0.4
kagi search --format pretty "obsidian cli daily notes workflow" | sed -n '1,12p'
sleep 2
