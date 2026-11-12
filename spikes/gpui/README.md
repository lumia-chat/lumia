# Spike: the desktop shell on GPUI

The same shell as [`spikes/rune`](../rune) (server rail, channel sidebar, a 10k-message list, a composer), over the same store, built on Zed's GPUI through [GPUI Kit](https://github.com/longbridge/gpui-kit). The numbers for both, side by side, are in [docs/spikes/2026-11-11-gpui-desktop.md](../../docs/spikes/2026-11-11-gpui-desktop.md). This is a spike, not a shell: it reads a seeded local store and never signs in.

## Run

```sh
PYTHON=python3 ../data/scripts/fonts.sh          # once: assets/fonts/*.woff2 to ../data/fonts/*.ttf (needs fonttools and brotli)
(cd ../data && cargo run --release --bin seed)   # once: 10 channels of 10,000 messages into ../data/store
cargo run --profile perf                         # the window
```

Headless, through GPUI's test platform with the real text system and an offscreen renderer:

```sh
cargo build --profile perf --features headless --bin bench
target/perf/bench snap out/dark.png dark
target/perf/bench snap out/bodies.png light bodies     # every body block and a line per script
```

## Measure

```sh
python3 ../data/scripts/window-startup.py target/perf/lumia-gpui-spike [--limit 200]   # launch to a window's first frame
target/perf/bench [scroll|fling|type|append|switch|top]                                # windowless frame costs
LUMIA_SPIKE_AUTOSCROLL=1 LUMIA_SPIKE_FRAMES=900 target/perf/lumia-gpui-spike           # a real window scrolling: frame spacing
cargo test --release                                                                   # composition, selection, the composer's role
```

`LUMIA_SPIKE_DATA` names the store directory, `LUMIA_SPIKE_LIMIT` how many of a channel's newest messages the list holds (10,000), `LUMIA_SPIKE_TIMING` prints store timings, `LUMIA_SPIKE_THEME` forces `dark` or `light`. `LUMIA_SPIKE_TREE=path` writes the accessibility tree once an assistive client has connected (GPUI builds none before that): turn VoiceOver on, launch with it set, read the file.

## Check by hand

The same checks as the Rune spike's README, in this window: a Japanese, Chinese or Korean input method in the composer, dead keys, the emoji picker and dictation; VoiceOver over the channel list, the message list and the composer; trackpad momentum and rubber-banding; text beside the SwiftUI app's.
