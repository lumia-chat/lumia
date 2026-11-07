//! The safety number two people compare (identity spec, section 2):
//! one digest over both identities' ids and current root keys, rendered
//! as sixty decimal digits in groups of five. It changes when either
//! side rotates its root, which is what the comparison is for.

/// The safety number for identities `a` and `b`, each given as its id
/// and its current root public key. Symmetric: the two inputs are sorted
/// before hashing, so both sides compute the same digits.
#[must_use]
pub fn safety_number(a: (&[u8], &[u8]), b: (&[u8], &[u8])) -> String {
    let mut inputs = [[a.0, a.1].concat(), [b.0, b.1].concat()];
    inputs.sort();
    let digest = blake3::hash(&[inputs[0].as_slice(), inputs[1].as_slice()].concat());
    digits(digest.as_bytes())
}

/// The digest as a big-endian integer modulo 10^60, as sixty digits
/// (leading zeros kept) in twelve groups of five.
fn digits(digest: &[u8; 32]) -> String {
    // Base conversion by hand: the integer kept in base 10^9 limbs, least
    // significant first, each byte folded in as x = x * 256 + byte.
    const BASE: u64 = 1_000_000_000;
    let mut limbs: Vec<u64> = vec![0];
    for &byte in digest {
        let mut carry = u64::from(byte);
        for limb in &mut limbs {
            let value = *limb * 256 + carry;
            *limb = value % BASE;
            carry = value / BASE;
        }
        while carry > 0 {
            limbs.push(carry % BASE);
            carry /= BASE;
        }
    }
    let mut decimal = String::new();
    for (i, limb) in limbs.iter().rev().enumerate() {
        if i == 0 {
            decimal.push_str(&limb.to_string());
        } else {
            decimal.push_str(&format!("{limb:09}"));
        }
    }
    let last60 = if decimal.len() > 60 {
        decimal[decimal.len() - 60..].to_string()
    } else {
        format!("{decimal:0>60}")
    };
    last60
        .as_bytes()
        .chunks(5)
        .map(|group| String::from_utf8_lossy(group).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::identity::keys::generate_signing_key;
    use crate::identity::roster::rotation;
    use prost::Message as _;

    fn identity(root: &ed25519_dalek::SigningKey) -> (Vec<u8>, Vec<u8>) {
        let public = root.verifying_key().to_bytes().to_vec();
        (blake3::hash(&public).as_bytes().to_vec(), public)
    }

    #[test]
    fn sixty_digits_in_twelve_groups_the_same_from_both_sides() {
        let a = identity(&generate_signing_key().unwrap());
        let b = identity(&generate_signing_key().unwrap());
        let ab = safety_number((&a.0, &a.1), (&b.0, &b.1));
        let ba = safety_number((&b.0, &b.1), (&a.0, &a.1));
        assert_eq!(ab, ba);
        let groups: Vec<&str> = ab.split(' ').collect();
        assert_eq!(groups.len(), 12);
        assert!(groups
            .iter()
            .all(|g| g.len() == 5 && g.bytes().all(|c| c.is_ascii_digit())));
    }

    #[test]
    fn the_rendering_is_the_digest_modulo_ten_to_the_sixty() {
        // 2^256 - 1 is 1157920892373161954 23570985008687907853269984665640564039457584007913129639935:
        // the last sixty digits, in fives.
        assert_eq!(
            digits(&[0xff; 32]),
            "42357 09850 08687 90785 32699 84665 64056 40394 57584 00791 31296 39935"
        );
        assert_eq!(
            digits(&[0; 32]),
            "00000 00000 00000 00000 00000 00000 00000 00000 00000 00000 00000 00000"
        );
    }

    /// Identity spec section 12: after a root rotation the safety number
    /// with anyone changes, while the identity id does not.
    #[test]
    fn a_root_rotation_changes_the_safety_number() {
        let old = generate_signing_key().unwrap();
        let new = generate_signing_key().unwrap();
        let (my_id, old_public) = identity(&old);
        let other = identity(&generate_signing_key().unwrap());
        let before = safety_number((&my_id, &old_public), (&other.0, &other.1));

        let rotated = rotation(
            &my_id,
            &old,
            &new.verifying_key().to_bytes(),
            prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            },
        );
        let body = crate::proto::RootRotationBody::decode(rotated.body.as_slice()).unwrap();
        let new_public = body.to.unwrap().ed25519;
        assert_eq!(
            body.identity_id, my_id,
            "the identity keeps its id across a rotation"
        );

        let after = safety_number((&my_id, &new_public), (&other.0, &other.1));
        assert_ne!(before, after);
        assert_eq!(
            after,
            safety_number((&other.0, &other.1), (&my_id, &new_public)),
            "the other side computes the same new number"
        );
    }
}
