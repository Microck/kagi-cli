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
printf '$ kagi translate "Bonjour tout le monde" --to ja | jq -M ...\n'
sleep 0.4
kagi translate "Bonjour tout le monde" --to ja \
  | jq -M '{
      detected_language: .detected_language.label,
      translation: .translation.translation,
      alignments: (.text_alignments.alignments | length),
      alternatives: (.alternatives.elements | map(.translation)[0:3]),
      suggestion_labels: (.translation_suggestions.suggestions | map(.label)[0:3]),
      word_insight_terms: (.word_insights.insights | map(.original_text)[0:3])
    }'
sleep 2
