#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"
unset KAGI_API_TOKEN

printf '\033c'
printf '$ cargo run --quiet -- search --pretty "rust programming language"\n'
cargo run --quiet -- search --pretty "rust programming language" | sed -n '1,12p'
sleep 1
