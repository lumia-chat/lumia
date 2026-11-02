//! End-to-end encryption for personal DMs (docs/specs/e2ee.md), on OpenMLS.
//!
//! * [`storage`] is the OpenMLS storage provider over a small key-value
//!   contract the transport implements inside its store transaction.
//! * [`credential`] is the leaf identity (`identity_id || device_id`) and
//!   the roster lookup every leaf is validated against (spec section 2).
//! * [`key_packages`] builds and validates key packages (section 3).
//! * [`group`] wraps one conversation's `MlsGroup` (sections 4 to 6, 10).
//! * [`projection`] turns decrypted operations into the message list
//!   (section 5).
//!
//! Nothing here does I/O: the transport claims key packages, sends the
//! commits and ciphertexts this module returns, and feeds back what the
//! log delivers.

pub mod credential;
pub mod group;
pub mod key_packages;
pub mod projection;
pub mod storage;

use openmls::prelude::Ciphersuite;
pub use openmls::prelude::KeyPackage;
pub use openmls_basic_credential::SignatureKeyPair;

/// The one ciphersuite v1 speaks (spec section 2).
pub const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// Key packages a device keeps unclaimed on each instance (spec section 3).
pub const KEY_PACKAGE_POOL: usize = 20;
/// Below this many the device tops up.
pub const KEY_PACKAGE_TOP_UP_BELOW: usize = 10;
/// Key package lifetime, in seconds (90 days).
pub const KEY_PACKAGE_LIFETIME_SECS: u64 = 90 * 24 * 60 * 60;

/// Why an MLS operation failed.
#[derive(Debug, thiserror::Error)]
pub enum MlsError {
    /// The storage provider failed.
    #[error("mls storage: {0}")]
    Storage(String),
    /// OpenMLS refused the operation.
    #[error("mls: {0}")]
    Library(String),
    /// A leaf failed validation against the rosters (spec section 2).
    #[error("leaf rejected: {0}")]
    LeafRejected(&'static str),
    /// Not the pinned ciphersuite.
    #[error("unsupported ciphersuite")]
    Ciphersuite,
    /// Bytes did not parse as the expected MLS or protobuf message.
    #[error("malformed: {0}")]
    Malformed(String),
    /// No group state for this conversation.
    #[error("no group for this conversation")]
    NoGroup,
    /// The message is for an epoch this group has not reached; hold it.
    #[error("message for epoch {epoch}, group at {current}")]
    FutureEpoch {
        /// The message's epoch.
        epoch: u64,
        /// The group's epoch.
        current: u64,
    },
    /// The message is from before this device joined (spec section 5).
    #[error("sent before this device joined")]
    BeforeJoin,
    /// The message is for the right epoch but did not decrypt.
    #[error("could not be decrypted")]
    Undecryptable,
    /// A commit is staged and must be merged or discarded first.
    #[error("a commit is pending")]
    PendingCommit,
}

/// Any OpenMLS error, by its debug form: the caller cannot act on the
/// variants beyond retrying or rejoining, so one variant carries them.
pub(crate) fn lib<E: std::fmt::Debug>(e: E) -> MlsError {
    MlsError::Library(format!("{e:?}"))
}
