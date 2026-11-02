//! The secret-store contract. The core never touches a platform secret
//! store directly; shells provide Keychain, Android Keystore, DPAPI, Secret
//! Service or WebCrypto implementations, and the CLI a file.

/// A named-secret store. Names are stable strings such as
/// `"instance:<host>:refresh_token"` or `"identity:<id>:device_key"`.
pub trait Keystore {
    /// Reads a secret, or `None` when absent.
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, KeystoreError>;
    /// Writes or replaces a secret.
    fn put(&mut self, name: &str, value: &[u8]) -> Result<(), KeystoreError>;
    /// Removes a secret; absent is not an error.
    fn delete(&mut self, name: &str) -> Result<(), KeystoreError>;
}

/// Why a keystore operation failed.
#[derive(Debug, thiserror::Error)]
pub enum KeystoreError {
    /// The platform store refused or is unavailable.
    #[error("keystore unavailable: {0}")]
    Unavailable(String),
    /// The store is present but the operation failed.
    #[error("keystore error: {0}")]
    Other(String),
}
