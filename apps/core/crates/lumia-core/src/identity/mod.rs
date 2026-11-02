//! The client side of docs/specs/identity.md.
//!
//! * [`phrase`]: the 24-word recovery phrase and the keys derived from it.
//! * [`keys`]: Ed25519 signing, X25519 agreement, XChaCha20-Poly1305,
//!   BLAKE3, and the signed-bytes helpers every roster entry uses.
//! * [`roster`]: building roster entries and the eight-step verifier,
//!   a port of `Lumia.Identity.Roster` checked against the shared vectors
//!   in `proto/vectors/identity/`.
//! * [`link`]: the link request a new device shows and the bundle an
//!   existing device encrypts for it.

pub mod keys;
pub mod link;
pub mod phrase;
pub mod roster;

/// Errors from identity operations other than roster verification.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    /// The recovery phrase is not 24 valid BIP39 words.
    #[error("invalid recovery phrase: {0}")]
    Phrase(String),
    /// A key or signature had the wrong length or shape.
    #[error("invalid key material: {0}")]
    Key(&'static str),
    /// Authenticated decryption failed.
    #[error("decryption failed")]
    Decrypt,
    /// Bytes did not decode as the expected message.
    #[error("undecodable {0}")]
    Decode(&'static str),
    /// The system's randomness source failed.
    #[error("no randomness available")]
    Randomness,
}
