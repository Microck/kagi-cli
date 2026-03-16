#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

printf '\033c'
printf '$ cargo run --quiet -- news --category world --limit 1 | jq -M ...\n'
cargo run --quiet -- news --category world --limit 1 \
  | jq -M '{
      category: .category.category_name,
      title: .stories[0].title,
      source_count: .stories[0].unique_domains,
      summary: .stories[0].short_summary
    }'
sleep 1
