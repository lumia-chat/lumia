# Spike: the desktop shell on Rune

Lumia's shell (server rail, channel sidebar, a 10k-message list, a composer) built on [Rune](https://github.com/anomalyco/rune) over the client core in-process. The question, the numbers and what they do not show are in [docs/spikes/2026-11-11-rune-desktop.md](../../docs/spikes/2026-11-11-rune-desktop.md); [`spikes/gpui`](../gpui) is the same shell on GPUI. This is a spike, not a shell: it reads a seeded local store and never signs in.

## Run

```sh
PYTHON=python3 ../data/scripts/fonts.sh          # once: assets/fonts/*.woff2 to ../data/fonts/*.ttf (needs fonttools and brotli)
(cd ../data && cargo run --release --bin seed)   # once: 10 channels of 10,000 messages into ../data/store
cargo run --profile perf                 # the window
```

Headless, through Rune's tooling flags (no window, same renderer):

```sh
B=target/perf/lumia-rune-spike
$B --snap out/dark.png --theme dark
$B --script scripts/scroll.json --fps 120 --video out/scroll.mp4   # needs ffmpeg
$B --tree out/tree.json                                            # elements and the accessibility tree
```

## Measure

```sh
python3 scripts/startup.py [--limit 200]                                              # launch to first pixels, offscreen
python3 ../data/scripts/window-startup.py target/perf/lumia-rune-spike [--limit 200]  # launch to a window's first frame
cargo run --profile perf --features profile --bin bench [scroll|fling|type|append|switch|top]
LUMIA_SPIKE_AUTOSCROLL=1 LUMIA_SPIKE_FRAMES=900 $B                                    # a real window scrolling: frame spacing
LUMIA_SPIKE_AUTOSCROLL=1 $B --native --trace out/native.jsonl --frames 900            # the same, with present times
cargo test --release                                                                  # composition and selection
```

`LUMIA_SPIKE_DATA` names the store directory, `LUMIA_SPIKE_LIMIT` how many of a channel's newest messages the list holds (10,000), `LUMIA_SPIKE_TIMING` prints store timings, `LUMIA_SPIKE_PLAIN` draws bodies as plain selectable text in one selection group.

## The kit patch

The kit's selectable Markdown is a selection of its own, so a drag stops at the end of a message. `patches/kit-selection-joined.patch` adds `Markdown::selection_joined`, which leaves a Markdown's texts to the group around it. To build against it, apply it to a Rune checkout at the pinned rev and point both crates there:

```sh
git -C $RUNE apply $PWD/patches/kit-selection-joined.patch
P=(--config "patch.'https://github.com/anomalyco/rune'.rune.path='$RUNE/crates/rune'" --config "patch.'https://github.com/anomalyco/rune'.rune-kit.path='$RUNE/crates/rune-kit'")
cargo test --release --features kit-patch "${P[@]}"                                    # selection across messages, through blocks, Cmd+A
cargo build --profile perf --features profile,kit-patch "${P[@]}" --bin bench
LUMIA_SPIKE_JOINED=1 target/perf/bench selectall                                       # the whole channel selected, then scrolled
```

That rewrites `Cargo.lock` to the path sources; restore it afterwards. `LUMIA_SPIKE_JOINED` turns the joined bodies on in a `kit-patch` build.

## Check by hand

What a headless run cannot show, in the window on macOS:

- A Japanese, Chinese or Korean input method in the composer: the candidate window sits at the caret, Return picks a candidate without sending, the marked text is underlined.
- Dead keys (Option-E then E), the emoji picker (Ctrl-Cmd-Space) and dictation.
- VoiceOver over the channel list, the message list ("12 of 10,000") and the composer.
- Trackpad scrolling: momentum, the rubber band at both ends, and whether fast scrolling ever shows a blank row.
- Text beside the SwiftUI app's at the same size: weight, spacing, emoji.
