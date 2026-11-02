//! Adding a device (docs/specs/identity.md section 4.2): the new device's
//! link request and the bundle an existing device encrypts to it.

use hkdf::Hkdf;
use prost::Message;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

use super::keys::{open, random, seal};
use super::IdentityError;
use crate::proto::{DeviceKind, LinkBundleBody, LinkRequest};

const LINK_INFO: &[u8] = b"lumia/link/v1";

/// The new device's half of the handshake: an ephemeral X25519 key and the
/// request it shows as a QR.
pub struct LinkRequester {
    ephemeral: StaticSecret,
    /// The request to display.
    pub request: LinkRequest,
}

impl std::fmt::Debug for LinkRequester {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkRequester")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

impl LinkRequester {
    /// Prepares a request for a device with the given id and signing key.
    ///
    /// # Errors
    /// When randomness is unavailable.
    pub fn new(
        device_id: Vec<u8>,
        device_public_key: Vec<u8>,
        kind: DeviceKind,
        label: &str,
        web_origin: &str,
    ) -> Result<Self, IdentityError> {
        let mut secret = [0u8; 32];
        random(&mut secret)?;
        let ephemeral = StaticSecret::from(secret);
        let mut nonce = vec![0u8; 32];
        random(&mut nonce)?;
        let request = LinkRequest {
            device_id,
            device_public_key: Some(crate::proto::PublicKey {
                ed25519: device_public_key,
            }),
            ephemeral_public_key: X25519Public::from(&ephemeral).to_bytes().to_vec(),
            kind: kind as i32,
            label: label.to_string(),
            web_origin: web_origin.to_string(),
            nonce,
        };
        Ok(Self { ephemeral, request })
    }

    /// The ephemeral secret, for a client that must persist the requester
    /// between showing the request and claiming the bundle.
    #[must_use]
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.ephemeral.to_bytes()
    }

    /// Rebuilds a requester from a persisted secret and request.
    ///
    /// # Errors
    /// When the secret is not 32 bytes.
    pub fn from_parts(secret: &[u8], request: LinkRequest) -> Result<Self, IdentityError> {
        let secret: [u8; 32] = secret
            .try_into()
            .map_err(|_| IdentityError::Key("secret must be 32 bytes"))?;
        Ok(Self {
            ephemeral: StaticSecret::from(secret),
            request,
        })
    }

    /// Decrypts the bundle an existing device produced for this request.
    ///
    /// # Errors
    /// When the key, nonce, or authentication fails, or the nonce echoed
    /// in the bundle does not match the request.
    pub fn open_bundle(
        &self,
        sender_ephemeral_public_key: &[u8],
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<LinkBundleBody, IdentityError> {
        let key = shared_key(&self.ephemeral, sender_ephemeral_public_key)?;
        let plain = open(&key, nonce, ciphertext)?;
        let bundle = LinkBundleBody::decode(plain.as_slice())
            .map_err(|_| IdentityError::Decode("LinkBundleBody"))?;
        if bundle.nonce != self.request.nonce {
            return Err(IdentityError::Decrypt);
        }
        Ok(bundle)
    }
}

/// What the existing device uploads: ciphertext, its ephemeral public key, the nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedBundle {
    /// XChaCha20-Poly1305 ciphertext of the serialized `LinkBundleBody`.
    pub ciphertext: Vec<u8>,
    /// The sender's ephemeral X25519 public key.
    pub sender_ephemeral_public_key: Vec<u8>,
    /// The 24-byte nonce.
    pub nonce: Vec<u8>,
}

/// The existing device's half: encrypts a bundle to a scanned request. The
/// bundle must echo `request.nonce`; this sets it.
///
/// # Errors
/// When the request's ephemeral key is malformed or randomness is unavailable.
pub fn seal_bundle(
    request: &LinkRequest,
    mut bundle: LinkBundleBody,
) -> Result<SealedBundle, IdentityError> {
    let mut secret = [0u8; 32];
    random(&mut secret)?;
    let ephemeral = StaticSecret::from(secret);
    let key = shared_key(&ephemeral, &request.ephemeral_public_key)?;
    bundle.nonce.clone_from(&request.nonce);
    let (nonce, ciphertext) = seal(&key, &bundle.encode_to_vec())?;
    Ok(SealedBundle {
        ciphertext,
        sender_ephemeral_public_key: X25519Public::from(&ephemeral).to_bytes().to_vec(),
        nonce,
    })
}

fn shared_key(secret: &StaticSecret, their_public: &[u8]) -> Result<[u8; 32], IdentityError> {
    let their: [u8; 32] = their_public
        .try_into()
        .map_err(|_| IdentityError::Key("ephemeral key must be 32 bytes"))?;
    let shared = secret.diffie_hellman(&X25519Public::from(their));
    let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut key = [0u8; 32];
    let _ = hk.expand(LINK_INFO, &mut key);
    Ok(key)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_round_trips_and_a_wrong_nonce_is_rejected() {
        let requester =
            LinkRequester::new(vec![1; 16], vec![2; 32], DeviceKind::Native, "laptop", "").unwrap();
        let bundle = LinkBundleBody {
            backup_key: vec![9; 32],
            ..Default::default()
        };
        let sealed = seal_bundle(&requester.request, bundle).unwrap();
        let opened = requester
            .open_bundle(
                &sealed.sender_ephemeral_public_key,
                &sealed.nonce,
                &sealed.ciphertext,
            )
            .unwrap();
        assert_eq!(opened.backup_key, vec![9; 32]);
        assert_eq!(opened.nonce, requester.request.nonce);

        let other =
            LinkRequester::new(vec![1; 16], vec![2; 32], DeviceKind::Native, "x", "").unwrap();
        assert!(other
            .open_bundle(
                &sealed.sender_ephemeral_public_key,
                &sealed.nonce,
                &sealed.ciphertext
            )
            .is_err());
    }
}
