//! The Lumia client core: everything that must be correct, written once,
//! with no I/O and no async (architecture section 13).
//!
//! * [`sync`] is the session state machine: frames in, effects out.
//! * [`identity`] is the client side of docs/specs/identity.md: phrase,
//!   keys, roster building and verification, link bundles.
//! * [`store`] and [`keystore`] are the contracts the transports implement.
//! * [`wire`] holds the scalar conventions from docs/protocol.md section 2.
//!
//! Transports (`lumia-client` natively, `lumia-wasm` in the browser) feed
//! this crate frames and carry out the effects it returns. Tests here run
//! against scripted frames, never a socket.

pub mod identity;
pub mod keystore;
pub mod mls;
pub mod store;
pub mod sync;
pub mod wire;

pub use lumia_proto::v1 as proto;
