//! The backup key after a root rotation, handed to the identity's other
//! native devices (identity spec, section 4.5 step 5): one copy sealed to
//! each device's key, the whole signed by the new root, carried to the
//! devices on their account logs. A device accepts a handoff only when
//! the root of the roster version it names, the one it holds, signed it.

use ed25519_dalek::{SigningKey, VerifyingKey};
use hkdf::Hkdf;
use prost::Message;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

use super::keys::{open, random, root_signature, seal, verify};
use super::IdentityError;
use crate::proto::{BackupKeyHandoff, BackupKeyHandoffBody, SealedBackupKey, SignerKind};

const HANDOFF_INFO: &[u8] = b"lumia identity backup-key handoff v1";

/// Seals `backup_key` to each of `devices` (device id, Ed25519 public
/// key) and signs the whole with `root`, the root that signed roster
/// `roster_version`.
///
/// # Errors
/// When a device key is malformed or randomness is unavailable.
pub fn seal_handoff(
    identity_id: &[u8],
    roster_version: u64,
    root: &SigningKey,
    backup_key: &[u8],
    devices: &[(Vec<u8>, Vec<u8>)],
    issued_at: prost_types::Timestamp,
) -> Result<BackupKeyHandoff, IdentityError> {
    let mut keys = Vec::with_capacity(devices.len());
    for (device_id, public) in devices {
        let recipient = x25519_of(public)?;
        let mut secret = [0u8; 32];
        random(&mut secret)?;
        let ephemeral = StaticSecret::from(secret);
        let key = shared_key(&ephemeral, &recipient);
        let (nonce, ciphertext) = seal(&key, backup_key)?;
        keys.push(SealedBackupKey {
            device_id: device_id.clone(),
            sender_ephemeral_public_key: X25519Public::from(&ephemeral).to_bytes().to_vec(),
            nonce,
            ciphertext,
        });
    }
    let body = BackupKeyHandoffBody {
        identity_id: identity_id.to_vec(),
        roster_version,
        keys,
        issued_at: Some(issued_at),
    }
    .encode_to_vec();
    Ok(BackupKeyHandoff {
        signature: Some(root_signature(root, &body)),
        body,
    })
}

/// Opens this device's copy of a handoff against the held roster's root
/// and version: `None` when the handoff holds none for `device_id`. A
/// handoff for a version older than the one held still opens when the
/// held root signed it (the roster moved on under the same root, as it
/// does when a device is added after the rotation); one for a newer
/// version waits for that roster.
///
/// # Errors
/// When the signature is not by `root_public_key`, the body names a
/// roster version past `roster_version`, or the copy does not open.
pub fn open_handoff(
    handoff: &BackupKeyHandoff,
    root_public_key: &[u8],
    roster_version: u64,
    device_id: &[u8],
    device_key: &SigningKey,
) -> Result<Option<Vec<u8>>, IdentityError> {
    let sig = handoff
        .signature
        .as_ref()
        .ok_or(IdentityError::Key("handoff has no signature"))?;
    if sig.signer_kind != SignerKind::Root as i32
        || sig.signer_id != root_public_key
        || !verify(&handoff.body, &sig.signature, root_public_key)
    {
        return Err(IdentityError::Key(
            "handoff is not signed by the current root",
        ));
    }
    let body = BackupKeyHandoffBody::decode(handoff.body.as_slice())
        .map_err(|_| IdentityError::Decode("BackupKeyHandoffBody"))?;
    if body.roster_version > roster_version {
        return Err(IdentityError::Key(
            "handoff is for a roster version not held yet",
        ));
    }
    let Some(mine) = body.keys.iter().find(|k| k.device_id == device_id) else {
        return Ok(None);
    };
    let secret = StaticSecret::from(device_key.to_scalar_bytes());
    let sender: [u8; 32] = mine
        .sender_ephemeral_public_key
        .as_slice()
        .try_into()
        .map_err(|_| IdentityError::Key("sender key must be 32 bytes"))?;
    let key = shared_key(&secret, &sender);
    open(&key, &mine.nonce, &mine.ciphertext).map(Some)
}

/// A device's Ed25519 public key read as X25519 (the Montgomery form).
fn x25519_of(ed25519_public: &[u8]) -> Result<[u8; 32], IdentityError> {
    let bytes: [u8; 32] = ed25519_public
        .try_into()
        .map_err(|_| IdentityError::Key("device key must be 32 bytes"))?;
    let key = VerifyingKey::from_bytes(&bytes)
        .map_err(|_| IdentityError::Key("device key is not an Ed25519 point"))?;
    Ok(key.to_montgomery().to_bytes())
}

fn shared_key(secret: &StaticSecret, their_public: &[u8; 32]) -> [u8; 32] {
    let shared = secret.diffie_hellman(&X25519Public::from(*their_public));
    let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut key = [0u8; 32];
    let _ = hk.expand(HANDOFF_INFO, &mut key);
    key
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::identity::keys::generate_signing_key;

    fn ts() -> prost_types::Timestamp {
        prost_types::Timestamp {
            seconds: 1_700_000_000,
            nanos: 0,
        }
    }

    #[test]
    fn each_device_opens_its_own_copy_and_a_stranger_has_none() {
        let root = generate_signing_key().unwrap();
        let root_public = root.verifying_key().to_bytes().to_vec();
        let phone = generate_signing_key().unwrap();
        let laptop = generate_signing_key().unwrap();
        let stranger = generate_signing_key().unwrap();
        let backup_key = [7u8; 32];
        let handoff = seal_handoff(
            b"identity",
            4,
            &root,
            &backup_key,
            &[
                (vec![1u8; 16], phone.verifying_key().to_bytes().to_vec()),
                (vec![2u8; 16], laptop.verifying_key().to_bytes().to_vec()),
            ],
            ts(),
        )
        .unwrap();

        assert_eq!(
            open_handoff(&handoff, &root_public, 4, &[1u8; 16], &phone).unwrap(),
            Some(backup_key.to_vec())
        );
        assert_eq!(
            open_handoff(&handoff, &root_public, 4, &[2u8; 16], &laptop).unwrap(),
            Some(backup_key.to_vec())
        );
        assert_eq!(
            open_handoff(&handoff, &root_public, 4, &[3u8; 16], &stranger).unwrap(),
            None
        );
        // The laptop's copy does not open with the phone's key.
        assert!(open_handoff(&handoff, &root_public, 4, &[2u8; 16], &phone).is_err());
    }

    #[test]
    fn another_root_or_another_version_is_refused() {
        let root = generate_signing_key().unwrap();
        let other_root = generate_signing_key().unwrap();
        let phone = generate_signing_key().unwrap();
        let handoff = seal_handoff(
            b"identity",
            4,
            &root,
            &[9u8; 32],
            &[(vec![1u8; 16], phone.verifying_key().to_bytes().to_vec())],
            ts(),
        )
        .unwrap();
        let other_public = other_root.verifying_key().to_bytes();
        assert!(open_handoff(&handoff, &other_public, 4, &[1u8; 16], &phone).is_err());
        let root_public = root.verifying_key().to_bytes();
        // Not yet at the version the handoff names: wait for the roster.
        assert!(open_handoff(&handoff, &root_public, 3, &[1u8; 16], &phone).is_err());
        // Past it under the same root: the key is still the current one.
        assert_eq!(
            open_handoff(&handoff, &root_public, 5, &[1u8; 16], &phone).unwrap(),
            Some(vec![9u8; 32])
        );
        let mut tampered = handoff.clone();
        tampered.body.push(0);
        assert!(open_handoff(&tampered, &root_public, 4, &[1u8; 16], &phone).is_err());
    }
}
