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

### The app on macOS

`cargo run` starts a bare binary: no icon, no `lumia:` links from the system (so no sign-in through the browser), no notifications. The bundle has them:

```sh
scripts/bundle-macos.sh                     # cargo build --release, then out/Lumia.app, signed ad hoc
scripts/bundle-macos.sh --debug             # the debug build, which honours LUMIA_DEV_*
LUMIA_SIGN_IDENTITY="Developer ID Application: Name (TEAM)" scripts/bundle-macos.sh    # hardened runtime, timestamp
open out/Lumia.app
```

The script checks what it made before it replaces the last bundle: the `Info.plist` lints, the signature verifies, and the bundled binary draws a frame headlessly (`out/bundle-check.png`). Its head says what it does not do (notarization, the DMG, the update feed).

- **One copy per data directory.** A second launch, however it is started, hands its `lumia:` links to the first and exits; a headless run (`--snap`, `--tree`, `--script`) over a directory in use is refused and says to set `LUMIA_DATA_DIR`.
- **The `lumia:` scheme.** macOS learns it from the bundle when the bundle is first opened, and gives the scheme to the app it registered last. While the SwiftUI app (or a build of it under `apps/apple`) is on the same Mac, both claim `lumia:`; `lsregister -u out/Lumia.app` (under `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/`) takes this bundle's claim back. `LUMIA_BUNDLE_ID=chat.lumia.desktop.scratch` makes a bundle whose preferences stay apart from the real one's.
- **The real launch, checked.** `LUMIA_DEV_APP=$PWD/out/Lumia.app LUMIA_DEV_INSTANCE=http://localhost:4000 cargo test --test live_app` opens a debug bundle twice for a few seconds, behind the app in front, and checks one copy, a link from the system and from a second launch signing the app in, the right to post notifications, and the saved frame. `live_browser` and `live_notice` (with `LUMIA_CLI`) check the handoff and a personal message's notice against the instance without a window.

## The Tauri shell

What ships on Windows and Linux until the Rune shell replaces it there: Tauri 2 showing the web UI from `../web` on the system webview, with the Rust core as WASM inside it, exactly as in the browser.

```sh
bun install
bun run dev        # vite for ../web, then the window on http://localhost:5173
bun run build      # ../web's production build, then the bundles under src-tauri/target/release/bundle
```

Needs the Rust toolchain pinned in `src-tauri/rust-toolchain.toml`, Tauri's platform prerequisites (on Linux, `libwebkit2gtk-4.1-dev` and friends; see `.github/workflows/desktop.yml`), and everything `../web/README.md` needs. A build also wants `TAURI_SIGNING_PRIVATE_KEY` for the updater archives; without the key, `bun tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'`.

Releases are GitHub Releases: a `v*` tag builds and attaches the bundles (`.github/workflows/release.yml`), and an installed app offers the next release at launch.
