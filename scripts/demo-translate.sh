#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."
. ./scripts/demo-common.sh

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"

build_demo_kagi

printf '\033c'
sleep 1.2
printf '$ kagi translate "Hello, how are you today?" --to es | jq -M ...\n'
sleep 0.4
"$KAGI_DEMO_BIN" translate "Hello, how are you today?" --to es \
  | jq -M '{
      detected_language: .detected_language.label,
      translation: .translation.translation,
      alignments: (.text_alignments.alignments | length),
      alternatives: (.alternatives.elements | map(.translation)[0:3]),
      suggestion_labels: (.translation_suggestions.suggestions | map(.label)[0:3]),
      word_insight_terms: (.word_insights.insights | map(.original_text)[0:3])
    }'
sleep 2
