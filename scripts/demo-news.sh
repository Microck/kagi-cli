#!/usr/bin/env bash
#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

cargo build --quiet
mkdir -p /tmp/kagi-demo-bin
ln -sf "$PWD/target/debug/kagi" /tmp/kagi-demo-bin/kagi
export PATH="/tmp/kagi-demo-bin:$PATH"

printf '\033c'
sleep 1.2
printf '$ kagi news --category tech --limit 1 | jq -M ...\n'
sleep 0.4
kagi news --category tech --limit 1 \
  | jq -M '{
      category: .category.category_name,
      title: .stories[0].title,
      source_count: .stories[0].unique_domains,
      summary: .stories[0].short_summary
    }'
sleep 2
