//! Key packages (spec section 3): building this device's, and validating
//! another device's before it is added to a group.

use openmls::prelude::tls_codec::{Deserialize, Serialize};
use openmls::prelude::{
    Capabilities, ExtensionType, KeyPackage, KeyPackageIn, Lifetime, MlsMessageBodyIn,
    MlsMessageIn, MlsMessageOut, ProtocolVersion,
};
use openmls_basic_credential::SignatureKeyPair;

use super::credential::{validate_leaf, LeafIdentity, RosterLookup};
use super::storage::{MlsKv, Provider};
use super::{lib, MlsError, SUITE};

/// Builds one key package for this device, serialized as an `MLSMessage`
/// of wire format `KeyPackage`. The private material is written to the
/// provider's storage and stays there until the package is used or
/// expires.
///
/// # Errors
/// When OpenMLS cannot build it or storage fails.
pub fn new_key_package<K: MlsKv>(
    provider: &Provider<K>,
    signer: &SignatureKeyPair,
    me: &LeafIdentity,
    lifetime_secs: u64,
    last_resort: bool,
) -> Result<Vec<u8>, MlsError> {
    let mut builder = KeyPackage::builder().key_package_lifetime(Lifetime::new(lifetime_secs));
    if last_resort {
        // The leaf must declare the extension it carries.
        builder = builder
            .mark_as_last_resort()
            .leaf_node_capabilities(Capabilities::new(
                None,
                None,
                Some(&[ExtensionType::LastResort]),
                None,
                None,
            ));
    }
    let bundle = builder
        .build(SUITE, provider, signer, me.credential(signer))
        .map_err(lib)?;
    MlsMessageOut::from(bundle.key_package().clone())
        .tls_serialize_detached()
        .map_err(lib)
}

/// Parses and validates a published key package: signature, lifetime,
/// ciphersuite, and the leaf against the rosters.
///
/// # Errors
/// [`MlsError::Malformed`] when the bytes are not a key package message,
/// [`MlsError::Ciphersuite`] for any suite but the pinned one,
/// [`MlsError::LeafRejected`] when the leaf fails roster validation.
pub fn parse_key_package<K: MlsKv>(
    provider: &Provider<K>,
    bytes: &[u8],
    lookup: &dyn RosterLookup,
) -> Result<(KeyPackage, LeafIdentity), MlsError> {
    let message = MlsMessageIn::tls_deserialize_exact(bytes)
        .map_err(|e| MlsError::Malformed(e.to_string()))?;
    let package: KeyPackageIn = match message.extract() {
        MlsMessageBodyIn::KeyPackage(kp) => kp,
        _ => return Err(MlsError::Malformed("not a key package".into())),
    };
    let package = package
        .validate(
            <Provider<K> as openmls_traits::OpenMlsProvider>::crypto(provider),
            ProtocolVersion::Mls10,
        )
        .map_err(lib)?;
    if package.ciphersuite() != SUITE {
        return Err(MlsError::Ciphersuite);
    }
    let leaf = validate_leaf(
        package.leaf_node().credential(),
        package.leaf_node().signature_key().as_slice(),
        lookup,
    )?;
    Ok((package, leaf))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::credential::device_signer;
    use super::super::storage::MemoryKv;
    use super::*;

    fn me() -> LeafIdentity {
        LeafIdentity {
            identity_id: vec![1; 32],
            device_id: vec![2; 16],
        }
    }

    #[test]
    fn a_key_package_round_trips_and_carries_last_resort() {
        let provider = Provider::new(MemoryKv::default());
        let signer = device_signer(&[3; 32]).unwrap();
        let public = signer.public().to_vec();
        let lookup = move |_: &[u8], _: &[u8]| Some(public.clone());

        let plain = new_key_package(&provider, &signer, &me(), 3600, false).unwrap();
        let (kp, leaf) = parse_key_package(&provider, &plain, &lookup).unwrap();
        assert_eq!(leaf, me());
        assert!(!kp.last_resort());

        let last = new_key_package(&provider, &signer, &me(), 3600, true).unwrap();
        let (kp, _) = parse_key_package(&provider, &last, &lookup).unwrap();
        assert!(kp.last_resort());
    }

    #[test]
    fn a_key_package_from_an_unrostered_device_is_rejected() {
        let provider = Provider::new(MemoryKv::default());
        let signer = device_signer(&[3; 32]).unwrap();
        let bytes = new_key_package(&provider, &signer, &me(), 3600, false).unwrap();
        let none = |_: &[u8], _: &[u8]| None;
        assert!(matches!(
            parse_key_package(&provider, &bytes, &none),
            Err(MlsError::LeafRejected(_))
        ));
        assert!(matches!(
            parse_key_package(&provider, b"junk", &none),
            Err(MlsError::Malformed(_))
        ));
    }
}
