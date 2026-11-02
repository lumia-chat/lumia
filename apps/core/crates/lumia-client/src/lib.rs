//! The Lumia client over the core (architecture section 13), for native
//! targets and for `wasm32-unknown-unknown` in a browser worker.
//!
//! * [`sqlite`] implements [`lumia_core::store::Store`], the MLS key-value
//!   store and a keystore on `rusqlite`: bundled SQLite natively, SQLite on
//!   the OPFS pool VFS on the web (docs/spikes/2026-11-02-web-store.md).
//! * [`api`] is the HTTP client for `/api/v1` with protobuf bodies, on
//!   reqwest's native or fetch backend.
//! * [`gateway`] drives a [`lumia_core::sync::Session`] over a socket,
//!   carrying out its effects against the store and the API; the socket
//!   and timers come from [`transport`] and [`time`], which are the only
//!   target-specific code.
//! * [`identity`] and [`dm`] are the flows over those.
//! * [`keystore`] is the development file keystore (native only).
//!
//! The headless client is the `lumia-cli` crate.

pub mod api;
pub mod dm;
pub mod gateway;
pub mod identity;
#[cfg(not(target_arch = "wasm32"))]
pub mod keystore;
pub mod sqlite;
pub mod time;
pub mod transport;

pub use futures_channel;
pub use futures_util;

/// Errors from the native layer.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The HTTP API answered with the protocol's error envelope.
    #[error("api error {code}: {message}")]
    Api {
        /// `lumia.v1.ErrorCode` as its name.
        code: String,
        /// The server's message.
        message: String,
        /// HTTP status.
        status: u16,
        /// `Error.details`, keyed as docs/protocol.md documents per code.
        details: std::collections::HashMap<String, String>,
    },
    /// This device is revoked in the roster it now holds (identity spec 5,
    /// "older roster": the instance returned the held version and the
    /// device adopted it).
    #[error("this device was revoked (roster v{roster_version})")]
    DeviceRevoked {
        /// The adopted roster's version.
        roster_version: u64,
    },
    /// Transport failure.
    #[error("transport: {0}")]
    Transport(String),
    /// Bytes did not decode.
    #[error("decode: {0}")]
    Decode(#[from] prost::DecodeError),
    /// The local store failed.
    #[error(transparent)]
    Store(#[from] lumia_core::store::StoreError),
    /// The keystore failed.
    #[error(transparent)]
    Keystore(#[from] lumia_core::keystore::KeystoreError),
    /// Not signed in to this instance.
    #[error("not signed in")]
    NotSignedIn,
    /// An identity operation failed.
    #[error(transparent)]
    Identity(#[from] lumia_core::identity::IdentityError),
    /// A roster was rejected.
    #[error(transparent)]
    Roster(#[from] lumia_core::identity::roster::RosterError),
    /// An MLS operation failed.
    #[error(transparent)]
    Mls(#[from] lumia_core::mls::MlsError),
    /// Required local state is missing.
    #[error("{0}")]
    State(String),
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e.to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<tokio_tungstenite::tungstenite::Error> for ClientError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::Transport(e.to_string())
    }
}
