# Lumia desktop

The desktop app for macOS, Windows and Linux (architecture section 13): one Rust program on [Rune](https://github.com/anomalyco/rune) with the client core linked in. Being built, macOS first, with no Windows or Linux package yet; [docs/plan.md](../../docs/plan.md) step 9 says what it does and does not do.

```sh
cargo run                                   # the window
cargo run -- --snap out/window.png          # the same frame headless, to a PNG (--theme dark, --tree out.json)
cargo test                                  # the screens without an instance
scripts/smoke.sh                            # against a dev instance (mix phx.server in ../instance), with the release CLI built in ../core
```

Needs the Rust toolchain in `rust-toolchain.toml` and nothing else: the fonts, icons and design tokens are compiled in from `../../assets` by `build.rs`. State lives in `chat.lumia` under the platform's application-data directory, or in `LUMIA_DATA_DIR`.

## The app on macOS

`cargo run` starts a bare binary: no icon, no `lumia:` links from the system (so no sign-in through the browser), no notifications, no updates. The bundle has them:

```sh
scripts/bundle-macos.sh                     # cargo build --release, then out/Lumia.app, signed ad hoc
scripts/bundle-macos.sh --debug             # the debug build, which honours LUMIA_DEV_*
LUMIA_SIGN_IDENTITY="Developer ID Application: Name (TEAM)" scripts/bundle-macos.sh    # hardened runtime, timestamp
open out/Lumia.app
```

The script checks what it made before it replaces the last bundle: the `Info.plist` lints, the signature verifies all the way down, the binary does not link Sparkle, and the bundled binary draws a frame headlessly (`out/bundle-check.png`). What a release adds (notarization, the disk image, the appcast) is `scripts/dist-macos.sh`'s.

- **One copy per data directory.** A second launch, however it is started, hands its `lumia:` links to the first and exits; a headless run (`--snap`, `--tree`, `--script`) over a directory in use is refused and says to set `LUMIA_DATA_DIR`.
- **The `lumia:` scheme.** macOS learns it from the bundle when the bundle is first opened, and gives the scheme to the app it registered last. A bundle built and opened here has the installed Lumia's identifier, `chat.lumia`, so it shares its preferences and takes `lumia:` from it; `lsregister -u out/Lumia.app` (under `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/`) takes this bundle's claim back. `LUMIA_BUNDLE_ID=chat.lumia.scratch` makes a bundle whose preferences and registration stay apart from the real one's.
- **The real launch, checked.** `LUMIA_DEV_APP=$PWD/out/Lumia.app LUMIA_DEV_INSTANCE=http://localhost:4000 cargo test --test live_app` opens a debug bundle twice for a few seconds, behind the app in front, and checks one copy, a link from the system and from a second launch signing the app in, the right to post notifications, and the saved frame. `live_browser` and `live_notice` (with `LUMIA_CLI`) check the handoff and a personal message's notice against the instance without a window.

## Updates on macOS

The bundled app updates itself with [Sparkle](https://sparkle-project.org) (architecture section 13, "Distribution"): once a day, and at a launch when a day has passed, it reads the appcast of the latest GitHub release, and offers what it finds in Sparkle's own windows. The application menu has "Check for updates…" and "Check for updates automatically", which turns the daily check off; that check is the one request the app makes to anything but the person's instances.

- **Where Sparkle comes from.** `scripts/bundle-macos.sh` fetches the release it pins (version and SHA-256 at its head; kept in `target/sparkle`, never committed), puts `Sparkle.framework` in `Contents/Frameworks` and signs inside out: Sparkle's two XPC services, its installer and its progress app, the framework, then the app. The shell loads the framework at run time (`crates/sparkle`, the one crate with `unsafe` in it), so the binary does not link it: a `cargo run` binary, the tests and a headless run have no updater, and their menu has no update items.
- **What it trusts.** `SUPublicEDKey` in the `Info.plist`, an Ed25519 public key. Every download must be signed with its private half, and so must the feed itself (`SURequireSignedFeed`, with `SUVerifyUpdateBeforeExtraction`): a download host that was taken over can neither push a build nor offer an old signed build as the newest. `LUMIA_UPDATE_FEED` and `LUMIA_UPDATE_KEY` set the feed and the key for a fork's builds; the defaults are the `appcast.xml` of this repository's latest release and the key its releases have always been signed with. `LUMIA_UPDATE_FEED=` (set, empty) makes a bundle with no updater and no Sparkle in it, which is the bundle to make for a test that only launches it.
- **A release.** `scripts/dist-macos.sh 0.3.0` is the macOS release path. It makes `out/dist`: `Lumia-0.3.0.dmg`, the disk image a person downloads and an update installs from; with `SPARKLE_KEY_PATH`, `appcast.xml`, signed; and, with the previous release's image in `LUMIA_PREVIOUS_DIR`, a delta that an app on that release downloads instead. With `LUMIA_SIGN_IDENTITY` the app and the image are signed with the Developer ID, and with `ASC_KEY_ID`, `ASC_ISSUER_ID` and `ASC_KEY_PATH` both are notarized and stapled; `--release` refuses to run without all of it. The signing and notarizing branch has not been run yet (its head says so), and `.github/workflows/release.yml` does not call the script yet.
- **Proved here.** `scripts/update-check.sh` runs the whole thing on this Mac with no hands on a window, under a scratch bundle identifier and throwaway keys of its own: an installed build finds a newer one on a feed served from this machine, downloads it, verifies it and installs it when the app quits (from the full disk image, and from a delta), and refuses a release signed with another key, an image signed with another key, an image changed after signing, an older release's feed edited to look newer, and a validly signed feed that offers an older build as a newer one. It opens the app seven times for a few seconds, behind the app in front, and cleans up after itself; `--debug` also reads the shell's trace for the menu's update items. What it cannot show is what a person sees: Sparkle's windows, and the two items in the menu bar.

## Windows and Linux

The same program builds for both, and a release tag publishes it for both as previews: nobody has run it on either desktop. What it lacks there is at the head of `src/platform/mod.rs` and in the `README.txt` each package carries (`packaging/`).

```sh
scripts/dist-linux.sh 0.3.0      # on Linux: out/dist/Lumia-0.3.0-linux-<arch>-preview.tar.gz and .deb
scripts/dist-windows.sh 0.3.0    # on Linux with MinGW-w64: out/dist/Lumia-0.3.0-windows-x86_64-preview.zip
```

- **Linux.** Needs `libxkbcommon-dev` and `libwayland-dev` to build, and a Vulkan driver to run: the shell draws with nothing else there, and its headless flags need one too (Mesa's `lvp` software driver will do, `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.*.json`). The `.deb` installs `/usr/bin/lumia`, a launcher entry that takes `lumia:` links, and the icon. Built on Ubuntu 22.04 it runs there and on anything newer.
- **Windows.** Cross-built from Linux (`rustup target add x86_64-pc-windows-gnu`, `gcc-mingw-w64-x86-64`), because that is what can be reproduced without a Windows machine. The zip holds `Lumia.exe` and nothing else it needs; there is no installer, no signature and no registered link type.
