//! The recovery phrase and derivation (docs/specs/identity.md section 2).
//!
//! 24 BIP39 English words encode 256 bits of entropy plus the checksum.
//! Keys derive from the entropy with HKDF-SHA256 under fixed labels; the
//! BIP39 PBKDF2 seed step is not used.

use bip39::{Language, Mnemonic};
use ed25519_dalek::SigningKey;
use hkdf::Hkdf;
use sha2::Sha256;

use super::keys::{blake3, random};
use super::IdentityError;

const ROOT_INFO: &[u8] = b"lumia/root/v1";
const BACKUP_INFO: &[u8] = b"lumia/backup/v1";

/// A recovery phrase.
#[derive(Clone)]
pub struct Phrase {
    mnemonic: Mnemonic,
}

impl std::fmt::Debug for Phrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Phrase(<redacted>)")
    }
}

/// What a phrase derives: the root and the backup key.
pub struct Derived {
    /// The Ed25519 root key.
    pub root: SigningKey,
    /// The 32-byte backup key for identity and history backups.
    pub backup_key: [u8; 32],
}

impl std::fmt::Debug for Derived {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Derived(<redacted>)")
    }
}

impl Phrase {
    /// A fresh 24-word phrase from 256 random bits.
    ///
    /// # Errors
    /// When randomness is unavailable.
    pub fn generate() -> Result<Self, IdentityError> {
        let mut entropy = [0u8; 32];
        random(&mut entropy)?;
        Self::from_entropy(&entropy)
    }

    /// A phrase from 32 bytes of entropy.
    ///
    /// # Errors
    /// When the entropy is not 32 bytes.
    pub fn from_entropy(entropy: &[u8]) -> Result<Self, IdentityError> {
        if entropy.len() != 32 {
            return Err(IdentityError::Key("entropy must be 32 bytes"));
        }
        Mnemonic::from_entropy_in(Language::English, entropy)
            .map(|mnemonic| Self { mnemonic })
            .map_err(|e| IdentityError::Phrase(e.to_string()))
    }

    /// Parses 24 words, tolerant of case and spacing.
    ///
    /// # Errors
    /// When the words are not a valid 24-word BIP39 phrase.
    pub fn parse(words: &str) -> Result<Self, IdentityError> {
        let normalized = words
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" ");
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, &normalized)
            .map_err(|e| IdentityError::Phrase(e.to_string()))?;
        if mnemonic.word_count() != 24 {
            return Err(IdentityError::Phrase(format!(
                "expected 24 words, got {}",
                mnemonic.word_count()
            )));
        }
        Ok(Self { mnemonic })
    }

    /// The words, space separated.
    #[must_use]
    pub fn words(&self) -> String {
        self.mnemonic.to_string()
    }

    /// The 32 bytes of entropy the words encode.
    #[must_use]
    pub fn entropy(&self) -> Vec<u8> {
        self.mnemonic.to_entropy()
    }

    /// Derives the root key and backup key.
    #[must_use]
    pub fn derive(&self) -> Derived {
        let entropy = self.entropy();
        let hk = Hkdf::<Sha256>::new(Some(b""), &entropy);
        let mut root_seed = [0u8; 32];
        let mut backup_key = [0u8; 32];
        // 32-byte outputs are always within HKDF's limit.
        let _ = hk.expand(ROOT_INFO, &mut root_seed);
        let _ = hk.expand(BACKUP_INFO, &mut backup_key);
        Derived {
            root: SigningKey::from_bytes(&root_seed),
            backup_key,
        }
    }
}

impl Derived {
    /// The identity id: BLAKE3-256 of the root public key.
    #[must_use]
    pub fn identity_id(&self) -> Vec<u8> {
        blake3(&self.root.verifying_key().to_bytes())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn generate_parse_and_derive_are_deterministic() {
        let phrase = Phrase::generate().unwrap();
        let words = phrase.words();
        assert_eq!(words.split(' ').count(), 24);
        let again = Phrase::parse(&words.to_uppercase()).unwrap();
        assert_eq!(again.entropy(), phrase.entropy());
        let a = phrase.derive();
        let b = again.derive();
        assert_eq!(a.root.to_bytes(), b.root.to_bytes());
        assert_eq!(a.backup_key, b.backup_key);
        assert_ne!(a.root.to_bytes(), a.backup_key);
        assert_eq!(a.identity_id().len(), 32);
    }

    #[test]
    fn known_entropy_gives_the_bip39_reference_words() {
        // The all-zero 256-bit vector from the BIP39 reference set.
        let phrase = Phrase::from_entropy(&[0u8; 32]).unwrap();
        assert!(phrase.words().starts_with("abandon abandon abandon"));
        assert!(phrase.words().ends_with("abandon art"));
    }

    #[test]
    fn wrong_word_counts_and_checksums_are_rejected() {
        assert!(Phrase::parse("abandon abandon abandon").is_err());
        let phrase = Phrase::from_entropy(&[7u8; 32]).unwrap();
        let mut words: Vec<&str> = phrase.words().leak().split(' ').collect();
        words[0] = "zoo";
        assert!(Phrase::parse(&words.join(" ")).is_err());
    }
}
