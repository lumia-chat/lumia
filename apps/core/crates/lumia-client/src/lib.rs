//! Native transport and store for the Lumia core (architecture section 13).
//!
//! * [`sqlite`] implements [`lumia_core::store::Store`] on `rusqlite`.
//! * [`api`] is the HTTP client for `/api/v1` with protobuf bodies.
//! * [`gateway`] drives a [`lumia_core::sync::Session`] over a WebSocket,
//!   carrying out its effects against the store and the API.
//! * [`keystore`] is the CLI's file keystore (plain file, 0600, dev only).
//!
//! The binary `lumia-cli` in `main.rs` is the headless client.

pub mod api;
pub mod gateway;
pub mod identity;
pub mod keystore;
pub mod sqlite;

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
    /// Required local state is missing.
    #[error("{0}")]
    State(String),
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e.to_string())
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for ClientError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::Transport(e.to_string())
    }
}
