//! Identity proofs (docs/specs/identity.md section 9, protocol section 13):
//! a device signs the exact challenge bytes a host issued, after checking
//! the challenge is bound to the host it is talking to.

use ed25519_dalek::SigningKey;
use prost::Message;

use super::keys::device_signature;
use super::IdentityError;
use crate::proto::{ChallengePurpose, IdentityChallengeBody, IdentityProof, SignedRoster};

/// Builds a proof for a challenge, refusing a challenge bound to another host.
///
/// # Errors
/// When the challenge does not decode, names a different host, or has a
/// different purpose than expected.
pub fn build_proof(
    challenge: &[u8],
    expected_host: &str,
    expected_purpose: ChallengePurpose,
    identity_id: &[u8],
    device_id: &[u8],
    device_key: &SigningKey,
    roster: SignedRoster,
) -> Result<IdentityProof, IdentityError> {
    let body = IdentityChallengeBody::decode(challenge)
        .map_err(|_| IdentityError::Decode("IdentityChallengeBody"))?;
    if body.host != expected_host {
        return Err(IdentityError::Key("challenge is bound to another host"));
    }
    if body.purpose != expected_purpose as i32 {
        return Err(IdentityError::Key("challenge has another purpose"));
    }
    Ok(IdentityProof {
        challenge: challenge.to_vec(),
        identity_id: identity_id.to_vec(),
        device_id: device_id.to_vec(),
        signature: Some(device_signature(device_id, device_key, challenge)),
        roster: Some(roster),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::identity::keys::{generate_signing_key, verify};

    fn challenge(host: &str, purpose: ChallengePurpose) -> Vec<u8> {
        IdentityChallengeBody {
            nonce: vec![1; 32],
            host: host.into(),
            purpose: purpose as i32,
            ..Default::default()
        }
        .encode_to_vec()
    }

    #[test]
    fn signs_the_exact_bytes_and_checks_host_and_purpose() {
        let key = generate_signing_key().unwrap();
        let bytes = challenge("chat.example", ChallengePurpose::SignIn);
        let proof = build_proof(
            &bytes,
            "chat.example",
            ChallengePurpose::SignIn,
            &[9; 32],
            &[1; 16],
            &key,
            SignedRoster::default(),
        )
        .unwrap();
        assert_eq!(proof.challenge, bytes);
        assert!(verify(
            &bytes,
            &proof.signature.unwrap().signature,
            &key.verifying_key().to_bytes()
        ));

        assert!(build_proof(
            &bytes,
            "evil.example",
            ChallengePurpose::SignIn,
            &[9; 32],
            &[1; 16],
            &key,
            SignedRoster::default()
        )
        .is_err());
        assert!(build_proof(
            &bytes,
            "chat.example",
            ChallengePurpose::CreateAccount,
            &[9; 32],
            &[1; 16],
            &key,
            SignedRoster::default()
        )
        .is_err());
    }
}
