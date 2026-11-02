# Lumia core

The Rust that every Lumia client shares (architecture section 13): the sync engine, the local store and keystore contracts, identity, and the native transport, plus `lumia-cli`, the headless client. Read [docs/architecture.md](../../docs/architecture.md) and [docs/protocol.md](../../docs/protocol.md) first; the conventions are in [AGENTS.md](AGENTS.md).

## Crates

| Crate | What it is |
|---|---|
| `lumia-proto` | Wire types generated from `../../proto` at build time (prost through protox; no `protoc` needed). |
| `lumia-core` | The sync state machine, `Store` and `Keystore` traits, identity, crypto. No I/O, no async. |
| `lumia-client` | tokio transport (WebSocket, HTTP), SQLite store, file keystore, and the `lumia-cli` binary. |
| `lumia-ffi` | UniFFI bindings for Swift and Kotlin. Scaffolded. |
| `lumia-wasm` | wasm-bindgen surface for the web shell. Scaffolded; checked on `wasm32-unknown-unknown` in CI. |

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

`tail` subscribes from the store's cursor, so a second run replays only what it missed. The CLI's keystore is a plaintext file at mode 0600 and is for development only.
