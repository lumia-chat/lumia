//! Primitives from docs/specs/identity.md section 2 and the signed-bytes
//! convention: everything signed is carried as the exact bytes signed.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::IdentityError;
use crate::proto::{self, SignerKind};

/// Fills `buf` from the system randomness source.
///
/// # Errors
/// When the source is unavailable.
pub fn random(buf: &mut [u8]) -> Result<(), IdentityError> {
    getrandom::fill(buf).map_err(|_| IdentityError::Randomness)
}

/// A fresh Ed25519 signing key.
///
/// # Errors
/// When randomness is unavailable.
pub fn generate_signing_key() -> Result<SigningKey, IdentityError> {
    let mut seed = [0u8; 32];
    random(&mut seed)?;
    Ok(SigningKey::from_bytes(&seed))
}

/// A signing key from a 32-byte seed.
///
/// # Errors
/// When the seed is not 32 bytes.
pub fn signing_key(seed: &[u8]) -> Result<SigningKey, IdentityError> {
    let seed: [u8; 32] = seed
        .try_into()
        .map_err(|_| IdentityError::Key("seed must be 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}

/// Ed25519 signature over `message`.
#[must_use]
pub fn sign(key: &SigningKey, message: &[u8]) -> Vec<u8> {
    key.sign(message).to_bytes().to_vec()
}

/// Whether `signature` is valid for `message` under `public` (32 bytes).
#[must_use]
pub fn verify(message: &[u8], signature: &[u8], public: &[u8]) -> bool {
    let Ok(public): Result<[u8; 32], _> = public.try_into() else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&public) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(signature) else {
        return false;
    };
    key.verify(message, &signature).is_ok()
}

/// BLAKE3-256.
#[must_use]
pub fn blake3(data: &[u8]) -> Vec<u8> {
    blake3::hash(data).as_bytes().to_vec()
}

/// A `Signature` message by the root, which identifies itself by public key.
#[must_use]
pub fn root_signature(root: &SigningKey, body: &[u8]) -> proto::Signature {
    proto::Signature {
        signer_kind: SignerKind::Root as i32,
        signer_id: root.verifying_key().to_bytes().to_vec(),
        signature: sign(root, body),
    }
}

/// A `Signature` message by a device.
#[must_use]
pub fn device_signature(device_id: &[u8], key: &SigningKey, body: &[u8]) -> proto::Signature {
    proto::Signature {
        signer_kind: SignerKind::Device as i32,
        signer_id: device_id.to_vec(),
        signature: sign(key, body),
    }
}

/// XChaCha20-Poly1305 with a fresh 24-byte nonce. Returns `(nonce, ciphertext)`.
///
/// # Errors
/// When the key is not 32 bytes or randomness is unavailable.
pub fn seal(key: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), IdentityError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| IdentityError::Key("key must be 32 bytes"))?;
    let mut nonce = [0u8; 24];
    random(&mut nonce)?;
    let xnonce =
        XNonce::try_from(&nonce[..]).map_err(|_| IdentityError::Key("nonce must be 24 bytes"))?;
    let ciphertext = cipher
        .encrypt(&xnonce, plaintext)
        .map_err(|_| IdentityError::Decrypt)?;
    Ok((nonce.to_vec(), ciphertext))
}

/// XChaCha20-Poly1305 decryption.
///
/// # Errors
/// When the key or nonce has the wrong length or authentication fails.
pub fn open(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, IdentityError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| IdentityError::Key("key must be 32 bytes"))?;
    let xnonce =
        XNonce::try_from(nonce).map_err(|_| IdentityError::Key("nonce must be 24 bytes"))?;
    cipher
        .decrypt(&xnonce, ciphertext)
        .map_err(|_| IdentityError::Decrypt)
}

/// Nanoseconds for a timestamp, 0 when absent. The verifier compares these.
#[must_use]
pub fn ts_nanos(ts: Option<&prost_types::Timestamp>) -> i128 {
    ts.map_or(0, |t| {
        i128::from(t.seconds) * 1_000_000_000 + i128::from(t.nanos)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_round_trip_and_reject_tampering() {
        let key = generate_signing_key().unwrap();
        let public = key.verifying_key().to_bytes();
        let sig = sign(&key, b"hello");
        assert!(verify(b"hello", &sig, &public));
        assert!(!verify(b"hellp", &sig, &public));
        assert!(!verify(b"hello", &sig, &[0u8; 32]));
        assert!(!verify(b"hello", &sig[..10], &public));
    }

    #[test]
    fn seal_and_open_round_trip() {
        let mut key = [0u8; 32];
        random(&mut key).unwrap();
        let (nonce, ct) = seal(&key, b"secret").unwrap();
        assert_eq!(open(&key, &nonce, &ct).unwrap(), b"secret");
        assert_eq!(open(&[1u8; 32], &nonce, &ct), Err(IdentityError::Decrypt));
    }

    #[test]
    fn blake3_matches_the_reference_vector() {
        assert_eq!(
            hex_of(&blake3(b"")),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    fn hex_of(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
