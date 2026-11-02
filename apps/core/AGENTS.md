This is the Lumia client core: the Rust that every client shares (architecture section 13). Read `../../docs/architecture.md` and `../../docs/protocol.md` before changing anything here.

## Layout

- `lumia-proto`: wire types generated from `../../proto` at build time by `prost` through `protox`. Nothing is hand-written and nothing is committed; the schemas are the source.
- `lumia-core`: the sync state machine, the `Store` and `Keystore` contracts, identity, crypto. **No I/O and no async.** Everything that must be correct lives here as pure functions and state machines that take frames and return effects.
- `lumia-client`: the transport (HTTP, the gateway loop), SQLite store, identity flows and DM engine, for native targets and wasm32. Only `transport.rs`, `time.rs` and the manifest's target tables differ per target; everything else is one code path. The wasm32 build needs `AR_wasm32_unknown_unknown` set to `llvm-ar` (README).
- `lumia-cli`: the headless client binary and the scenario suite.
- `lumia-ffi` (UniFFI) and `lumia-wasm` (wasm-bindgen): marshalling only, no logic.

## Rules

- **Sans-I/O in the core.** `lumia-core` never opens a socket, a file, or a database, and never spawns a task. A transport feeds it `ServerFrame`s and carries out the `Effect`s it returns. If you need I/O to test something in the core, the design is wrong.
- `#![deny(unsafe_code)]` is a workspace lint; only a binding crate may relax it, with a comment saying why.
- No `unwrap` or `expect` outside tests (workspace lints deny them). Errors are `thiserror` enums with a variant per cause a caller can act on.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` must pass; CI runs them plus a wasm32 check of `lumia-client` and `lumia-wasm`.
- Wire scalars follow `docs/protocol.md` section 2: UUIDs are 16 bytes, timestamps are `prost_types::Timestamp`; use `lumia_core::wire` rather than converting inline.
- The store applies events and the cursor in one transaction, and refuses non-contiguous events. Do not add a path that writes one without the other. A snapshot path (`apply_snapshot`, `apply_account_snapshot`) is the one place a cursor moves without events, and it drops the local events it supersedes.
- A transport carries out the effects a `Session` returns in the order it returns them (a FIFO queue, never a stack). A fetched page is several `Apply` effects the store accepts only in sequence, and `Welcome` asks for the account log before the channels.
- A client-minted id is UUIDv7 from `wire::new_id()`; the server mints everything else.
- Secrets go through the `Keystore` trait. The CLI's file keystore is plaintext at mode 0600 and is for development only; say so wherever it is used.
- Pinned toolchain in `rust-toolchain.toml`; no nightly features.
