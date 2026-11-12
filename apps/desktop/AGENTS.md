This directory holds two desktop shells (architecture section 13). Read `../../docs/architecture.md` and `../../docs/plan.md` (steps 6 and 9) before changing anything here.

- **The Rune shell**, the crate at this directory's root: the desktop app for macOS, Windows and Linux, in Rust on [Rune](https://github.com/anomalyco/rune) with the client core linked in. It is being built, macOS first.
- **The Tauri shell**, `src-tauri/` with `package.json`: the web UI from `../web` on the system webview. It is what ships on Windows and Linux until the Rune shell reaches parity there, and is then removed.

The two share nothing: separate Cargo workspaces, toolchains and lock files.

# The Rune shell

## Layout

- `Cargo.toml`: its own workspace on Rust 1.99.1 and edition 2024, Rune's pins. Rune and `rune-kit` are git dependencies at one pinned rev, and the manifest repeats Rune's two `[patch.crates-io]` entries (its patched `wgpu-hal` and `winit`), which a dependency cannot apply for the workspace that uses it. Bump the four revs together. The core is a path dependency on `../core/crates/lumia-client`; there is no binding layer.
- `build.rs`: turns the shared design assets into what the shell compiles in, as `apps/apple/scripts/tokens.py` does for Swift. `assets/design/tokens.json` becomes `tokens.rs` in `OUT_DIR` (every colour for light and dark as linear `Color`s, the shadows as `Shadow` layers, spacing, radii and sizes as points, the type styles), the WOFF2 fonts the tokens list become TrueType (Rune reads no WOFF2), and `assets/icons` and the brand symbol become Rune `Icon`s. Nothing generated is committed.
- `src/session/`: the client and the state the screens read, the counterpart of LumiaKit's `Session.swift`. It owns a tokio runtime; calls that reach the network run there and come back as `Update`s in a queue the window drains on its own thread (`Session::poll`), woken through a callback. Reads of the local store are synchronous. `mod.rs` is the account, its servers and the conversation on screen (a server's channel or a personal conversation, both read as `Entry`s); `dms.rs`, `servers.rs` and `identity.rs` each hold one part of the product and report through their own update type (`Update::Dm`, `Update::Server`, `Update::Identity`). **No toolkit type appears in this directory**: architecture 13 names GPUI as the fallback, and this is the part a move would keep.
- `src/app/`: the window's state over the session and its reactions to what the session reports. `mod.rs` is the frame: the route, the place (Home with a `HomeView`, or a server), the open popover, the draft, and `init`, `setup` and `poll` for Rune's `AppSpec`. `sign_in.rs`, `home.rs`, `server.rs` and `account.rs` hold each part's forms and its `react_*`. `State::receive` applies an update as if the instance had sent it, which is how tests stand in for one. A call whose answer belongs to a form and not to the session (an invite's preview, say) still travels as an `Update`: the session applies nothing and the form's `react_*` takes it.
- `src/ui/`: the views. `components.rs` is the design system's pieces (avatar, card, field, buttons, segmented choices, notice, panel, popover). `shell.rs` is the Quiet Split's frame and the rail; `server.rs` fills it in a server (channels, members) and `home/` at Home (one module per `HomeView`). `conversation.rs` is the panel a channel and a personal conversation share (header, timeline, composer) and `timeline.rs` its messages. A popover is returned from `ui::overlay` so it mounts at the root. Views read the state and call its methods; they never touch the runtime.
- `src/tokens.rs`: includes the generated tokens and adds what is derived from them (the avatar tone an id hashes to, the Markdown style for message bodies).
- `src/settings.rs`: the data directory (`LUMIA_DATA_DIR`, else `chat.lumia.desktop` under the platform's application-data directory) and `settings.json` (the instance to reopen). The stores under it follow `lumia-ffi`'s layout: `<host>/store.sqlite`, `<host>/keys.sqlite`, `identity/`. It is deliberately not the SwiftUI macOS app's directory: two processes must not open one MLS store.
- `tests/shell.rs`: the screens through a windowless `rune::Runtime`, with no instance; `tests/common/` is what such tests share (a state, the gestures, what an instance would report). `tests/live.rs`: the session against a real instance, run by the smoke script (it skips itself without the `LUMIA_DEV_*` environment).
- `scripts/smoke.sh`: seeds a dev instance with the release CLI and checks the debug shell against it: signed in, snapshot loaded, gateway connected, messages and members in the tree, a send stored, a second launch reading from SQLite. It draws `out/smoke/shell.png` and `shell-dark.png` headlessly through Rune's `--snap`, which needs no window and no screen-recording permission; that is how a change to a screen is checked against the design canvas.

## Rules

- Logic lives in the Rust core. A call a screen needs goes on `lumia_client::app::App` first; `session.rs` holds state and the views hold layout.
- Colours, distances, radii, shadows and type come from `tokens`; no literal values in views. Sentence case, typographic punctuation, no emoji in chrome.
- The screens mirror the web's (`apps/web/src/routes`) and the SwiftUI app's: the same states, wording and error handling.
- Every interactive element has a role, a label, a focus ring and a key; a list is a `list` of `listitem`s. The accessibility tree is part of the screen (`--tree out.json` shows it).
- A message is text someone else wrote. Its links open only as `http` or `https`, and the shell fetches nothing a message names (no images, no previews).
- No `unwrap` or `expect` outside tests, and no `unsafe` (manifest lints). `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` must pass, and `scripts/smoke.sh` against a dev instance after a change to a screen or the session.
- What the shell does not do yet is listed in plan step 9. Say so in the screen's module comment when a flow is missing, and do not draw a control for it.

# The Tauri shell

## Layout

- `src-tauri/`: the Rust crate, its own Cargo workspace so Tauri's dependency tree stays out of `apps/core`. `lib.rs` builds and runs the app; `main.rs` is the binary entry. `tauri.conf.json` points dev at vite on :5173 and production at `../web/dist`. `capabilities/default.json` is what the window may ask of the shell: `core:default` only. `icons/` is generated from `assets/brand/lumia-original-icon.png` by `tauri icon`.
- Updates (architecture section 13, "Distribution"): `lib.rs` checks the release feed in `tauri.conf.json` (`plugins.updater`: GitHub's `latest.json`, the minisign public key) once at launch and asks before installing; `bundle.createUpdaterArtifacts` makes the signed archives, so a build needs `TAURI_SIGNING_PRIVATE_KEY` (CI's secret; `desktop.yml` turns the artifacts off instead). The version in `tauri.conf.json` is a placeholder; `.github/workflows/release.yml` writes the tag's on a release.
- The UI, the bridge and the core are `../web`'s; nothing of the product lives here. The page knows it runs in the shell (`__TAURI_INTERNALS__`) and tells the worker, which makes the device NATIVE rather than WEB (identity spec).
- The webview's origin is `tauri://localhost` on macOS and Linux and `https://tauri.localhost` on Windows; the instance accepts those for CORS and the gateway (`LumiaWeb.Plugs.Cors`), never as web-device origins.

## Rules

- Add a Tauri permission only with the feature that needs it, in `capabilities/default.json`, named in the commit.
- `cargo fmt`, `cargo clippy -- -D warnings` in `src-tauri` must pass; CI builds the bundles on Linux and Windows.
- Keep the shell thin: a feature that needs the shell's help (a tray, a notification, a file dialog) is a Tauri command here and a call through the bridge in `../web`, with the logic in the core.
