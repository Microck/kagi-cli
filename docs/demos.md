# Demo Assets

These terminal demos are recorded from the real CLI with `asciinema` and exported as GIFs for GitHub rendering with the official asciinema `agg` binary.

## Assets

- `docs/demo-assets/search.gif`
- `docs/demo-assets/summarize.gif`
- `docs/demo-assets/news.gif`
- `docs/demo-assets/assistant.gif`

## Regenerate

Subscriber demos require `KAGI_SESSION_TOKEN` in the environment. API-token demos are intentionally excluded here because the current account has zero billable API credit and the paid API surfaces reject upstream.

```bash
chmod +x scripts/demo-search.sh scripts/demo-summarize.sh scripts/demo-news.sh scripts/demo-assistant.sh

mkdir -p docs/demo-assets /tmp/kagi-demos

agg --version
# expected: "asciinema gif generator"

KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-search.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/search.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-summarize.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/summarize.cast
asciinema rec -c ./scripts/demo-news.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/news.cast
KAGI_SESSION_TOKEN='...' asciinema rec -c ./scripts/demo-assistant.sh -q -i 0.2 --cols 92 --rows 22 /tmp/kagi-demos/assistant.cast

agg --theme asciinema --font-size 14 --idle-time-limit 1 --last-frame-duration 1 /tmp/kagi-demos/search.cast docs/demo-assets/search.gif
agg --theme asciinema --font-size 14 --idle-time-limit 1 --last-frame-duration 1 /tmp/kagi-demos/summarize.cast docs/demo-assets/summarize.gif
agg --theme asciinema --font-size 14 --idle-time-limit 1 --last-frame-duration 1 /tmp/kagi-demos/news.cast docs/demo-assets/news.gif
agg --theme asciinema --font-size 14 --idle-time-limit 1 --last-frame-duration 1 /tmp/kagi-demos/assistant.cast docs/demo-assets/assistant.gif
```

If `agg --version` does not print `asciinema gif generator`, your `PATH` is resolving a different package. Use the official binary explicitly, for example `~/.cargo/bin/agg`.
