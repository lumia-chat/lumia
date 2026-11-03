This is Lumia on Apple platforms (architecture section 13): `LumiaKit`, the Swift package the macOS and iOS apps share, and one app directory per platform with its own layout. Read `../../docs/architecture.md` and `../../docs/plan.md` (step 7) before changing anything here.

## Layout

- `scripts/core.sh`: builds `lumia-ffi` for the Apple targets (`--ios` adds device and simulator), generates the Swift bindings with the workspace's `uniffi-bindgen`, assembles `LumiaKit/LumiaCore.xcframework`, and runs `scripts/tokens.py`, which writes `LumiaKit/Sources/LumiaKit/Tokens.swift` from `../../assets/design/tokens.json`. Everything it produces is ignored by git; run it before any build, with `DEVELOPER_DIR` pointing at Xcode.
- `LumiaKit/Sources/LumiaFFI`: the generated bindings (`LumiaFFI.swift`), a target of their own so their warnings stay out of ours.
- `LumiaKit/Sources/LumiaKit`: `LumiaClient` (an actor over the FFI `Client`; every call off the main thread, the listener as an `AsyncStream`), `LumiaSession` (`@Observable`, main actor: the snapshot, messages, DM channels and logs, the roster, the people, the connection state, kept fresh by the gateway's events the way the web's queries are), `Design.swift` and the generated `Tokens.swift` (colours per appearance, spacing, radii, type styles), `Presentation.swift` (initials, the nine avatar tones, times) and `Components.swift` (avatar, notice, empty state, message row, request card, device row, composer, card, field, button styles).
- `Lumia-macOS`: the desktop app. `project.yml` is the XcodeGen definition (`xcodegen generate`; the project is not committed); `Sources` holds `LumiaApp` (the store that reopens the saved instance, routes between sign-in, link and the app), `SignInView`, `LinkView`, `ShellView` (the Quiet Split: rail, sidebar, main, aside; the channel view) and `HomeViews` (conversations, requests, new message, devices).
- `scripts/release.sh`: a Release archive of the macOS app, universal, signed automatically with the team in `project.yml`; `--upload` exports it to App Store Connect for TestFlight (`Lumia-macOS/ExportOptions.plist`). The build number is the commit count. Xcode reaches the account through a signed-in Apple ID or an App Store Connect API key in `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_PATH`.
- `scripts/smoke.sh`: seeds a dev instance with the release CLI, launches the debug app signed in through the `LUMIA_DEV_*` environment (a debug build only), and checks the window and the gateway connection.

## Rules

- Logic lives in the Rust core; `LumiaKit` holds state and presentation, the apps hold layouts. A method a screen needs goes on `lumia_client::app::App` first, then through `lumia-ffi`.
- Colours, spacing, radii and type come from `LumiaTokens`; no literal values in views. Sentence case, typographic punctuation, no emoji in chrome.
- The screens mirror the web's (`apps/web/src/routes`): the same states, wording and error handling, so a change to one is a change to both.
- Build and test: `scripts/core.sh`, then `swift test` in `LumiaKit` and `xcodebuild -project Lumia-macOS/Lumia.xcodeproj -scheme Lumia build` (after `xcodegen generate`). Run `scripts/smoke.sh` against a dev instance after a change to a screen or the session.
