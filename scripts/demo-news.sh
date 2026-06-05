#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
. ./scripts/demo-common.sh

build_demo_kagi

printf '\033c'
sleep 1.2
printf '$ kagi news --category tech --limit 1 | jq -M ...\n'
sleep 0.4
"$KAGI_DEMO_BIN" news --category tech --limit 1 \
  | jq -M '{
      category: .category.category_name,
      title: .stories[0].title,
      source_count: .stories[0].unique_domains,
      summary: .stories[0].short_summary
    }'
sleep 2
