# Lumia desktop

Two shells live here (architecture section 13).

## The Rune shell

The desktop app for macOS, Windows and Linux: one Rust program on [Rune](https://github.com/anomalyco/rune) with the client core linked in. Being built, macOS first; [docs/plan.md](../../docs/plan.md) step 9 says what it does and does not do yet.

```sh
cargo run                                   # the window
cargo run -- --snap out/window.png          # the same frame headless, to a PNG (--theme dark, --tree out.json)
cargo test                                  # the screens without an instance
scripts/smoke.sh                            # against a dev instance (mix phx.server in ../instance), with the release CLI built in ../core
```

Needs the Rust toolchain in `rust-toolchain.toml` and nothing else: the fonts, icons and design tokens are compiled in from `../../assets` by `build.rs`. State lives in `chat.lumia.desktop` under the platform's application-data directory, or in `LUMIA_DATA_DIR`.

## The Tauri shell

What ships on Windows and Linux until the Rune shell replaces it there: Tauri 2 showing the web UI from `../web` on the system webview, with the Rust core as WASM inside it, exactly as in the browser.

```sh
bun install
bun run dev        # vite for ../web, then the window on http://localhost:5173
bun run build      # ../web's production build, then the bundles under src-tauri/target/release/bundle
```

Needs the Rust toolchain pinned in `src-tauri/rust-toolchain.toml`, Tauri's platform prerequisites (on Linux, `libwebkit2gtk-4.1-dev` and friends; see `.github/workflows/desktop.yml`), and everything `../web/README.md` needs. A build also wants `TAURI_SIGNING_PRIVATE_KEY` for the updater archives; without the key, `bun tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'`.

Releases are GitHub Releases: a `v*` tag builds and attaches the bundles (`.github/workflows/release.yml`), and an installed app offers the next release at launch.
