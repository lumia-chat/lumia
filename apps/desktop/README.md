# Lumia desktop

The desktop shell for Windows and Linux (and macOS while the SwiftUI app does not exist): Tauri 2 showing the web UI from `../web` on the system webview, with the Rust core as WASM inside it, exactly as in the browser.

```sh
bun install
bun run dev        # vite for ../web, then the window on http://localhost:5173
bun run build      # ../web's production build, then the bundles under src-tauri/target/release/bundle
```

Needs the Rust toolchain pinned in `src-tauri/rust-toolchain.toml`, Tauri's platform prerequisites (on Linux, `libwebkit2gtk-4.1-dev` and friends; see `.github/workflows/desktop.yml`), and everything `../web/README.md` needs. A build also wants `TAURI_SIGNING_PRIVATE_KEY` for the updater archives; without the key, `bun tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'`.

Releases are GitHub Releases: a `v*` tag builds and attaches the bundles (`.github/workflows/release.yml`), and an installed app offers the next release at launch.
