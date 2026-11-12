# The desktop spikes' store

What [`spikes/rune`](../rune) and [`spikes/gpui`](../gpui) both read, so the two toolkits draw the same channels: a real `lumia-client` store seeded through `Store::apply`, opened through `lumia_client::app::App`, and a `manifest.json` beside it for the channel list the core does not keep locally.

```sh
cargo run --release --bin seed [--channels 10] [--messages 10000] [--dir PATH]   # into store/
PYTHON=python3 scripts/fonts.sh                                                   # assets/fonts/*.woff2 to fonts/*.ttf
python3 scripts/window-startup.py ../rune/target/perf/lumia-rune-spike            # launch to a window's first frame, either spike
```
