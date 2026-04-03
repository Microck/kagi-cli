# Demo Assets

These terminal demos are recorded from the real CLI with `asciinema` and exported as GIFs for GitHub rendering with the official asciinema `agg` binary.

The demo scripts build the local debug binary, expose it as `kagi` through `/tmp/kagi-demo-bin`, and add short intro and outro pauses so the commands are readable in the rendered GIFs.

## Assets

- `docs/demo-assets/auth.gif`
- `docs/demo-assets/search.gif`
- `docs/demo-assets/quick.gif`
- `docs/demo-assets/summarize.gif`
- `docs/demo-assets/news.gif`
- `docs/demo-assets/ask-page.gif`
- `docs/demo-assets/assistant.gif`
- `docs/demo-assets/translate.gif`
- `docs/demo-assets/lens.gif`
- `docs/demo-assets/bang-custom.gif`
- `docs/demo-assets/redirect.gif`
- `docs/demo-assets/assistant-custom.gif`

## Regenerate

Subscriber demos require `KAGI_SESSION_TOKEN` in the environment. API-token demos are intentionally excluded here because the current account has zero billable API credit and the paid API surfaces reject upstream.

The current demo commands are:

- `kagi auth`
- `kagi search --format pretty --region us --time year --order recency "rust release notes"`
- `kagi quick --format pretty "what is rust"`
- `kagi summarize --subscriber --url https://mullvad.net/en/browser | jq -M ...`
- `kagi news --category tech --limit 1 | jq -M ...`
- `kagi ask-page https://rust-lang.org/ "What is this page about in one sentence?" | jq -M ...`
- `kagi assistant "plan a private obsidian workflow for cafe work. give me 3 setup tips and a short checklist." | jq -M ...`
- `kagi translate "Bonjour tout le monde" --to ja | jq -M ...`
- `RESPONSE=$(kagi assistant --model gpt-5-mini "..."); THREAD_ID=...; kagi assistant --thread-id "$THREAD_ID" "..."; kagi assistant thread export "$THREAD_ID"`
- `kagi lens list | jq -M ...`
- `kagi bang custom list | jq -M ...`
- `kagi redirect list | jq -M ...`
- `kagi assistant custom list | jq -M ...`

```bash
chmod +x scripts/demo-auth.sh scripts/demo-search.sh scripts/demo-quick.sh scripts/demo-summarize.sh scripts/demo-news.sh scripts/demo-ask-page.sh scripts/demo-assistant.sh scripts/demo-translate.sh scripts/demo-lens.sh scripts/demo-bang-custom.sh scripts/demo-redirect.sh scripts/demo-assistant-custom.sh

mkdir -p docs/demo-assets /tmp/kagi-demos

agg --version
# expected: "asciinema gif generator"

KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-auth.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/auth.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-search.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/search.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-quick.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/quick.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-summarize.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/summarize.cast
asciinema rec -c ./scripts/demo-news.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/news.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-ask-page.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/ask-page.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-assistant.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/assistant.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-translate.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/translate.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-lens.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/lens.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-bang-custom.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/bang-custom.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-redirect.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/redirect.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-assistant-custom.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/assistant-custom.cast

agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/auth.cast docs/demo-assets/auth.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/search.cast docs/demo-assets/search.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/quick.cast docs/demo-assets/quick.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/summarize.cast docs/demo-assets/summarize.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/news.cast docs/demo-assets/news.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/ask-page.cast docs/demo-assets/ask-page.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/assistant.cast docs/demo-assets/assistant.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/translate.cast docs/demo-assets/translate.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/lens.cast docs/demo-assets/lens.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/bang-custom.cast docs/demo-assets/bang-custom.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/redirect.cast docs/demo-assets/redirect.gif
agg --theme asciinema --font-size 14 --idle-time-limit 2 --last-frame-duration 4 /tmp/kagi-demos/assistant-custom.cast docs/demo-assets/assistant-custom.gif
```

If `agg --version` does not print `asciinema gif generator`, your `PATH` is resolving a different package. Use the official binary explicitly, for example `~/.cargo/bin/agg`.
