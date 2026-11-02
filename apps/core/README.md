# Lumia core

The Rust that every Lumia client shares (architecture section 13): the sync engine, the local store and keystore contracts, identity, and the native transport, plus `lumia-cli`, the headless client. Read [docs/architecture.md](../../docs/architecture.md) and [docs/protocol.md](../../docs/protocol.md) first; the conventions are in [AGENTS.md](AGENTS.md).

## Crates

| Crate | What it is |
|---|---|
| `lumia-proto` | Wire types generated from `../../proto` at build time (prost through protox; no `protoc` needed). |
| `lumia-core` | The sync state machine, `Store` and `Keystore` traits, identity, crypto, and `mls`: OpenMLS behind a storage provider over a key-value contract, leaf validation against rosters, and the DM projection. No I/O, no async. |
| `lumia-client` | tokio transport (WebSocket, HTTP), SQLite store (also the MLS key-value store, in the same transaction), file keystore, the personal-DM engine, and the `lumia-cli` binary. |
| `lumia-ffi` | UniFFI surface for Swift and Kotlin: one `Client` per instance (API, reconnecting gateway loop, store, identity) and a `Listener` callback. Blocking methods, dev store and keystore until the shells bring their own. |
| `lumia-wasm` | wasm-bindgen surface for the web shell: the sans-I/O `Session` as a JS class, effects as plain objects, the store and socket owned by JS. Checked on `wasm32-unknown-unknown` in CI. |

## Requirements

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
cargo run -p lumia-client -- instance add http://127.0.0.1:4000
cargo run -p lumia-client -- account create --handle ryan --password 'correct horse battery'
cargo run -p lumia-client -- server-create Acme
cargo run -p lumia-client -- channel-create <server-id> general
cargo run -p lumia-client -- send <channel-id> "hello"
cargo run -p lumia-client -- tail <channel-id>
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
lumia-cli identity recover --phrase '...'        # no device left: revoke them all, add this one
lumia-cli identity status                        # the roster version each instance holds
lumia-cli identity sync                          # publish this device's roster everywhere
```

Personal DMs (specs/e2ee.md) are end-to-end encrypted with MLS; the device that opens a conversation creates the group and welcomes every other device of both identities, and key packages are topped up on connect:

```sh
lumia-cli dm setup                               # publish key packages now; prints the account id
lumia-cli dm open <account-id>                   # open the DM; the opener creates the group
lumia-cli dm tail <channel-id>                   # join from the welcome, decrypt as events arrive
lumia-cli dm send <channel-id> "hello"
lumia-cli dm messages <channel-id>               # the projection this device holds
lumia-cli dm backup on|off|status                # history backup on this instance (spec section 9)
lumia-cli dm restore                             # replay every device's backup into this one
lumia-cli dm rejoin <channel-id>                 # after recovery: external commit from GroupInfo
```

A device revoked from the roster is removed from every group by the first device that learns of it, and refused by the others from the moment they hold the new roster. A newly linked device is added to every group by the first member that sees it in the roster and finds its key packages.

`scenario <name>` (or `all`) runs the failure-injection cases a client can drive on its own against the instance (delivery spec 12, identity spec 12): `token-rotation`, `frame-before-hello`, `duplicate-command`, `snapshot-under-writes`, `snapshot-threshold`, `stale-roster`, `device-revoke`, `roster-rules`, `reconnect`. Each sets up a throwaway server or identity and fails with what it saw.

## Identity vectors

`proto/vectors/identity/rosters.json` holds rosters with the outcome the spec requires. The Elixir suite writes it and checks its verifier; `crates/lumia-core/tests/identity_vectors.rs` checks the Rust verifier against the same bytes. To regenerate after changing the fixtures or the proto:

```sh
cd ../instance && LUMIA_WRITE_VECTORS=1 mix test test/lumia/identity/vectors_test.exs
```
