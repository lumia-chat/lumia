//! Leaf identities and their validation against rosters (spec section 2).
//!
//! A leaf's basic credential is `identity_id (32 bytes) || device_id (16
//! bytes)` and its signature key is the device key from the roster. The
//! roster certificate is therefore what authenticates a leaf; a
//! [`RosterLookup`] answers, for a conversation, whether a device is an
//! authorized device of a participant and with which key.

use ed25519_dalek::SigningKey;
use openmls::prelude::{BasicCredential, Credential, CredentialWithKey, SignatureScheme};
use openmls_basic_credential::SignatureKeyPair;

use super::MlsError;

/// Who a leaf claims to be.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LeafIdentity {
    /// The identity id (32 bytes).
    pub identity_id: Vec<u8>,
    /// The device id (16 bytes).
    pub device_id: Vec<u8>,
}

impl LeafIdentity {
    /// The credential's identity bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48);
        out.extend_from_slice(&self.identity_id);
        out.extend_from_slice(&self.device_id);
        out
    }

    /// Parses credential identity bytes.
    ///
    /// # Errors
    /// When the bytes are not 48 long.
    pub fn parse(bytes: &[u8]) -> Result<Self, MlsError> {
        if bytes.len() != 48 {
            return Err(MlsError::LeafRejected("credential is not identity||device"));
        }
        Ok(Self {
            identity_id: bytes[..32].to_vec(),
            device_id: bytes[32..].to_vec(),
        })
    }

    /// The credential and signature public key for this leaf.
    #[must_use]
    pub fn credential(&self, signer: &SignatureKeyPair) -> CredentialWithKey {
        CredentialWithKey {
            credential: BasicCredential::new(self.encode()).into(),
            signature_key: signer.public().into(),
        }
    }
}

/// What the transport knows about participants' devices: the device key for
/// an authorized device of a participant, or nothing. "Nothing" rejects the
/// leaf; a transport that holds no roster for a participant fetches one
/// before judging (spec section 2).
pub trait RosterLookup {
    /// The Ed25519 public key certified for `device_id` under
    /// `identity_id`, when the identity is a participant and the device is
    /// in its roster and not revoked.
    fn device_key(&self, identity_id: &[u8], device_id: &[u8]) -> Option<Vec<u8>>;
}

impl<F: Fn(&[u8], &[u8]) -> Option<Vec<u8>>> RosterLookup for F {
    fn device_key(&self, identity_id: &[u8], device_id: &[u8]) -> Option<Vec<u8>> {
        self(identity_id, device_id)
    }
}

/// Checks a leaf's credential and signature key against the rosters.
///
/// # Errors
/// [`MlsError::LeafRejected`] with the reason: not a basic credential, not
/// identity||device, unknown or revoked device, or a key that is not the
/// device's certificate key.
pub fn validate_leaf(
    credential: &Credential,
    signature_key: &[u8],
    lookup: &dyn RosterLookup,
) -> Result<LeafIdentity, MlsError> {
    let basic = BasicCredential::try_from(credential.clone())
        .map_err(|_| MlsError::LeafRejected("not a basic credential"))?;
    let leaf = LeafIdentity::parse(basic.identity())?;
    let expected = lookup
        .device_key(&leaf.identity_id, &leaf.device_id)
        .ok_or(MlsError::LeafRejected(
            "device not in a participant's roster",
        ))?;
    if expected.as_slice() != signature_key {
        return Err(MlsError::LeafRejected(
            "signature key is not the device key",
        ));
    }
    Ok(leaf)
}

/// The MLS signer for a device key seed (the 32-byte Ed25519 secret the
/// identity flows keep in the keystore).
///
/// # Errors
/// When the seed is not 32 bytes.
pub fn device_signer(seed: &[u8]) -> Result<SignatureKeyPair, MlsError> {
    let seed: [u8; 32] = seed
        .try_into()
        .map_err(|_| MlsError::Malformed("device key seed".into()))?;
    let key = SigningKey::from_bytes(&seed);
    Ok(SignatureKeyPair::from_raw(
        SignatureScheme::ED25519,
        seed.to_vec(),
        key.verifying_key().to_bytes().to_vec(),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trips_and_rejects_bad_lengths() {
        let me = LeafIdentity {
            identity_id: vec![1; 32],
            device_id: vec![2; 16],
        };
        assert_eq!(LeafIdentity::parse(&me.encode()).unwrap(), me);
        assert!(LeafIdentity::parse(&[0; 47]).is_err());
    }

    #[test]
    fn a_leaf_is_accepted_only_with_the_rostered_key() {
        let me = LeafIdentity {
            identity_id: vec![1; 32],
            device_id: vec![2; 16],
        };
        let signer = device_signer(&[7; 32]).unwrap();
        let cred = me.credential(&signer);
        let public = signer.public().to_vec();

        let lookup = |_: &[u8], _: &[u8]| Some(public.clone());
        assert_eq!(
            validate_leaf(&cred.credential, signer.public(), &lookup).unwrap(),
            me
        );

        let other = device_signer(&[8; 32]).unwrap();
        let wrong = |_: &[u8], _: &[u8]| Some(other.public().to_vec());
        assert!(validate_leaf(&cred.credential, signer.public(), &wrong).is_err());

        let unknown = |_: &[u8], _: &[u8]| None;
        assert!(validate_leaf(&cred.credential, signer.public(), &unknown).is_err());
    }
}
