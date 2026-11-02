This is the Lumia desktop shell (architecture section 13): Tauri 2 around the web UI in `../web`. Read `../../docs/architecture.md` and `../../docs/plan.md` (step 6) before changing anything here.

## Layout

- `src-tauri/`: the Rust crate, its own Cargo workspace so Tauri's dependency tree stays out of `apps/core`. `lib.rs` builds and runs the app; `main.rs` is the binary entry. `tauri.conf.json` points dev at vite on :5173 and production at `../web/dist`. `capabilities/default.json` is what the window may ask of the shell: `core:default` only. `icons/` is generated from `assets/brand/lumia-original-icon.png` by `tauri icon`.
- The UI, the bridge and the core are `../web`'s; nothing of the product lives here. The page knows it runs in the shell (`__TAURI_INTERNALS__`) and tells the worker, which makes the device NATIVE rather than WEB (identity spec).
- The webview's origin is `tauri://localhost` on macOS and Linux and `https://tauri.localhost` on Windows; the instance accepts those for CORS and the gateway (`LumiaWeb.Plugs.Cors`), never as web-device origins.

## Rules

- Add a Tauri permission only with the feature that needs it, in `capabilities/default.json`, named in the commit.
- `cargo fmt`, `cargo clippy -- -D warnings` in `src-tauri` must pass; CI builds the bundles on Linux and Windows.
- Keep the shell thin: a feature that needs the shell's help (a tray, a notification, a file dialog) is a Tauri command here and a call through the bridge in `../web`, with the logic in the core.
