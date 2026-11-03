# Lumia core

The Rust that every Lumia client shares (architecture section 13): the sync engine, the local store and keystore contracts, identity, and the native transport, plus `lumia-cli`, the headless client. Read [docs/architecture.md](../../docs/architecture.md) and [docs/protocol.md](../../docs/protocol.md) first; the conventions are in [AGENTS.md](AGENTS.md).

## Crates

| Crate | What it is |
|---|---|
| `lumia-proto` | Wire types generated from `../../proto` at build time (prost through protox; no `protoc` needed). |
| `lumia-core` | The sync state machine, `Store` and `Keystore` traits, identity, crypto, and `mls`: OpenMLS behind a storage provider over a key-value contract, leaf validation against rosters, and the DM projection. No I/O, no async. |
| `lumia-client` | The client for native targets and `wasm32-unknown-unknown`: the HTTP client, the gateway loop over a per-target socket and timer, the SQLite store (also the MLS key-value store and a keystore, in the same transaction; bundled SQLite natively, SQLite on the OPFS pool VFS on the web), the identity flows and the personal-DM engine. |
| `lumia-cli` | The headless client binary and the scenario suite, on `lumia-client`. |
| `lumia-ffi` | UniFFI surface for Swift and Kotlin over `lumia_client::app::App`: one `Client` per instance (API, reconnecting gateway loop on its own Tokio runtime, SQLite store and keystore under the shell's data directory, identity, push, DMs) and a `Listener` callback. Blocking methods; the Swift side wraps them in an actor. |
| `uniffi-bindgen` | The `uniffi-bindgen` binary pinned to this workspace's UniFFI; `apps/apple/scripts/core.sh` runs it against the built library. |
| `lumia-wasm` | wasm-bindgen surface for the web shell over the same `App`: `Client`, one per instance, runs in a worker with the store on the OPFS pool VFS, the identity flows and the gateway loop, and delivers events as plain objects; the sans-I/O `Session` is also exposed for the store-less tests. Checked on `wasm32-unknown-unknown` in CI; `apps/web` builds it. |

## Requirements

On the web, `lumia-wasm` builds SQLite as SQLite3 Multiple Ciphers (`sqlite-wasm-rs` with `sqlite3mc`) and opens every store through its shim over the OPFS pool VFS with a raw key (`SqliteStore::open_on_vfs_keyed`), so the files are ciphertext at rest; `apps/web/scripts/wasm.ts` leaves out every cipher but ChaCha20.

The wasm32 build compiles SQLite with the `cc` crate, which needs an archiver that understands wasm objects: install the `llvm-tools` component and point `AR_wasm32_unknown_unknown` at its `llvm-ar` (`$(rustc --print sysroot)/lib/rustlib/<host>/bin/llvm-ar`); Apple's `ar` writes an empty archive and the link fails with undefined `sqlite3_*` symbols. The C compiler needs a WebAssembly target too: Apple's clang has one from Xcode 27; on an older Xcode point `CC_wasm32_unknown_unknown` at Homebrew's LLVM `clang` (the macOS CI job does).


Rust 1.96 (pinned in `rust-toolchain.toml`; rustup and mise pick it up). No other tools.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p lumia-wasm --target wasm32-unknown-unknown
```

## lumia-cli

Talks to a running instance (see [apps/instance](../instance/README.md)). State lives under the platform data directory, or `--data-dir`.

```sh
cargo run -p lumia-cli -- instance add http://127.0.0.1:4000
cargo run -p lumia-cli -- account create --handle ryan --password 'correct horse battery'
cargo run -p lumia-cli -- server-create Acme
cargo run -p lumia-cli -- channel-create <server-id> general
cargo run -p lumia-cli -- send <channel-id> "hello"
cargo run -p lumia-cli -- tail <channel-id>
```

`tail` subscribes from the store's cursor, so a second run replays only what it missed, and reconnects with backoff when the connection drops. An access token the instance refuses is refreshed once and the new pair persisted. The CLI's keystore is a plaintext file at mode 0600 and is for development only.

Identity (specs/identity.md) lives under `<data-dir>/identity/`, shared by every instance the data directory knows:

```sh
lumia-cli identity create --label phone          # prints the recovery phrase once
lumia-cli account create-by-identity --handle ryan
lumia-cli login --identity
lumia-cli identity link-request --label laptop   # on the new device; paste the output to:
lumia-cli identity approve <request>             # on an existing device; prints a code for:
lumia-cli identity claim <code>                  # on the new device
lumia-cli identity revoke <device-id> --phrase '...'       # root-signed
lumia-cli identity revoke <device-id> --password '...'     # device-signed, reauth by password
lumia-cli identity link-account --password '...'           # bind a password account to the identity
lumia-cli identity recover --phrase '...'        # no device left: revoke them all, add this one, read the backup
lumia-cli identity backup                        # write the identity backup here (sync writes it everywhere)
lumia-cli identity status                        # the roster version each instance holds
lumia-cli identity sync                          # publish this device's roster everywhere
```

Personal DMs (specs/e2ee.md) are end-to-end encrypted with MLS; the device that opens a conversation creates the group and welcomes every other device of both identities, and key packages are topped up on connect:

```sh
lumia-cli dm setup                               # publish key packages now; prints the account id
lumia-cli dm request <account-id>                # ask for a DM; they answer with:
lumia-cli dm requests                            # pending requests to this account
lumia-cli dm respond <request-id> accept         # the accepter creates the group (or decline, block)
lumia-cli dm open <account-id> [<account-id>...] # the pair's DM, or a new group DM; needs accepted requests
lumia-cli dm tail <channel-id>                   # join from the welcome, decrypt as events arrive
lumia-cli dm send <channel-id> "hello"
lumia-cli dm delete <channel-id> <message-id>    # a deleted message also leaves the backup
lumia-cli dm messages <channel-id>               # the projection this device holds
lumia-cli dm backup on|off|status                # history backup on this instance (spec section 9)
lumia-cli dm restore                             # replay every device's backup into this one
lumia-cli dm rejoin <channel-id>                 # after recovery: external commit from GroupInfo
```

A device revoked from the roster is removed from every group by the first device that learns of it, and refused by the others from the moment they hold the new roster. A newly linked device is added to every group by the first member that sees it in the roster and finds its key packages.

`scenario <name>` (or `all`) runs the failure-injection cases a client can drive on its own against the instance (delivery spec 12, identity spec 12): `token-rotation`, `frame-before-hello`, `duplicate-command`, `snapshot-under-writes`, `snapshot-threshold`, `stale-roster`, `device-revoke`, `roster-rules`, `last-resort`, `reconnect`. Each sets up a throwaway server or identity and fails with what it saw.

## Identity vectors

`proto/vectors/identity/rosters.json` holds rosters with the outcome the spec requires. The Elixir suite writes it and checks its verifier; `crates/lumia-core/tests/identity_vectors.rs` checks the Rust verifier against the same bytes. To regenerate after changing the fixtures or the proto:

```sh
cd ../instance && LUMIA_WRITE_VECTORS=1 mix test test/lumia/identity/vectors_test.exs
```
