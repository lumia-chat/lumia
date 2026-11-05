//! Roster building and verification (docs/specs/identity.md sections 3
//! and 4). The verifier is a port of `Lumia.Identity.Roster`'s eight steps
//! and is checked against the shared vectors in `proto/vectors/identity/`.

use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;
use prost::Message;

use super::keys::{blake3, device_signature, root_signature, ts_nanos, verify};
use crate::proto::{
    DeviceCertBody, DeviceEntry, DeviceKind, PublicKey, RevocationBody, RevokedEntry, RootRotation,
    RootRotationBody, RosterBody, Signature, SignedRoster, SignerKind,
};

const MAX_DEVICES: usize = 20;
const MAX_REVOKED: usize = 100;
const MAX_BYTES: usize = 65_536;
const CLOCK_SKEW_NANOS: i128 = 300 * 1_000_000_000;

/// Which verification step rejected a roster (spec section 3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// 1: parse, mirrors, limits.
    Parse,
    /// 2: identity id against the first root.
    Identity,
    /// 3: rotation chain.
    Chain,
    /// 4: device certificates.
    Certs,
    /// 5: revocations.
    Revocations,
    /// 6: roster signature.
    Signature,
    /// 7: monotonicity against the held version.
    Monotonic,
}

impl Step {
    /// The step name as the Elixir implementation and the vectors spell it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Identity => "identity",
            Self::Chain => "chain",
            Self::Certs => "certs",
            Self::Revocations => "revocations",
            Self::Signature => "signature",
            Self::Monotonic => "monotonic",
        }
    }
}

/// A rejected roster: the step and a reason spelled as the vectors spell it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("roster rejected at {}: {reason}", step.name())]
pub struct RosterError {
    /// The verification step.
    pub step: Step,
    /// The reason, such as `mirror_mismatch` or `future_dated`.
    pub reason: &'static str,
}

fn reject(step: Step, reason: &'static str) -> RosterError {
    RosterError { step, reason }
}

/// An authorized device as the verifier sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Its Ed25519 public key.
    pub public_key: Vec<u8>,
    /// Its kind.
    pub kind: DeviceKind,
    /// Web origin, for WEB devices.
    pub web_origin: String,
    /// When it was added, nanoseconds.
    pub added_at: i128,
    /// The certificate as carried.
    pub cert: DeviceCertBody,
}

/// A revoked device as the verifier sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokedInfo {
    /// Its Ed25519 public key, from the revocation body.
    pub public_key: Vec<u8>,
    /// When it was added, nanoseconds.
    pub added_at: i128,
    /// When it was revoked, nanoseconds.
    pub revoked_at: i128,
}

/// A roster that passed verification.
#[derive(Debug, Clone)]
pub struct Verified {
    /// The identity id.
    pub identity_id: Vec<u8>,
    /// The roster version.
    pub version: u64,
    /// The current root public key.
    pub root_public_key: Vec<u8>,
    /// Every root in the chain, oldest first.
    pub roots: Vec<Vec<u8>>,
    /// Authorized devices by id.
    pub devices: BTreeMap<Vec<u8>, DeviceInfo>,
    /// Revoked devices by id.
    pub revoked: BTreeMap<Vec<u8>, RevokedInfo>,
    /// Whether a valid root signature was present.
    pub root_signed: bool,
    /// Device ids revoked in this version but not in the held one.
    pub new_revocations: Vec<Vec<u8>>,
    /// The decoded body.
    pub body: RosterBody,
    /// The roster as carried.
    pub signed: SignedRoster,
}

/// Verifies a roster against an optional held version at time `now_nanos`.
///
/// # Errors
/// The step and reason of the first failing check.
pub fn verify_roster(
    signed: &SignedRoster,
    held: Option<&Verified>,
    now_nanos: i128,
) -> Result<Verified, RosterError> {
    let (body, devices, revoked) = parse(&signed.body)?;
    let first_root = identity(&body)?;
    let roots = chain(&body, &first_root)?;
    certs(&body, &devices, &revoked, &roots, now_nanos)?;
    revocations(&body, &revoked, &devices, &roots)?;
    let root_signed = signature(&signed.body, &signed.signatures, &body, &devices)?;
    let new_revocations = monotonic(&body, &devices, &revoked, held)?;

    Ok(Verified {
        identity_id: body.identity_id.clone(),
        version: body.version,
        root_public_key: body
            .root_public_key
            .as_ref()
            .map(|k| k.ed25519.clone())
            .unwrap_or_default(),
        roots,
        devices,
        revoked,
        root_signed,
        new_revocations,
        body,
        signed: signed.clone(),
    })
}

// ---- 1. Parse ----------------------------------------------------------------------

type Parsed = (
    RosterBody,
    BTreeMap<Vec<u8>, DeviceInfo>,
    BTreeMap<Vec<u8>, RevokedInfo>,
);

fn parse(bytes: &[u8]) -> Result<Parsed, RosterError> {
    if bytes.len() > MAX_BYTES {
        return Err(reject(Step::Parse, "too_large"));
    }
    let body = RosterBody::decode(bytes).map_err(|_| reject(Step::Parse, "undecodable"))?;
    if body.devices.len() > MAX_DEVICES {
        return Err(reject(Step::Parse, "too_many_devices"));
    }
    if body.revoked.len() > MAX_REVOKED {
        return Err(reject(Step::Parse, "too_many_revocations"));
    }
    if body.version < 1 {
        return Err(reject(Step::Parse, "version"));
    }
    if !key_ok(body.root_public_key.as_ref()) {
        return Err(reject(Step::Parse, "root_key"));
    }

    let mut devices = BTreeMap::new();
    for entry in &body.devices {
        let cert = DeviceCertBody::decode(entry.cert_body.as_slice())
            .map_err(|_| reject(Step::Parse, "undecodable"))?;
        if !mirrors_match(entry, &cert) {
            return Err(reject(Step::Parse, "mirror_mismatch"));
        }
        if devices.contains_key(&cert.device_id) {
            return Err(reject(Step::Parse, "duplicate_device"));
        }
        let public_key = cert
            .public_key
            .as_ref()
            .map(|k| k.ed25519.clone())
            .unwrap_or_default();
        devices.insert(
            cert.device_id.clone(),
            DeviceInfo {
                public_key,
                kind: DeviceKind::try_from(cert.kind).unwrap_or(DeviceKind::Unspecified),
                web_origin: cert.web_origin.clone(),
                added_at: ts_nanos(cert.added_at.as_ref()),
                cert,
            },
        );
    }

    let mut revoked = BTreeMap::new();
    for entry in &body.revoked {
        let rb = RevocationBody::decode(entry.body.as_slice())
            .map_err(|_| reject(Step::Parse, "undecodable"))?;
        let same = entry.device_id == rb.device_id
            && entry.revoked_at == rb.revoked_at
            && entry.revoked_by == rb.revoked_by;
        if !same {
            return Err(reject(Step::Parse, "mirror_mismatch"));
        }
        if !key_ok(rb.public_key.as_ref()) {
            return Err(reject(Step::Parse, "revoked_key"));
        }
        revoked.insert(
            rb.device_id.clone(),
            RevokedInfo {
                public_key: rb
                    .public_key
                    .as_ref()
                    .map(|k| k.ed25519.clone())
                    .unwrap_or_default(),
                added_at: ts_nanos(rb.added_at.as_ref()),
                revoked_at: ts_nanos(rb.revoked_at.as_ref()),
            },
        );
    }

    if revoked.keys().any(|id| devices.contains_key(id)) {
        return Err(reject(Step::Parse, "device_both_active_and_revoked"));
    }
    Ok((body, devices, revoked))
}

fn key_ok(key: Option<&PublicKey>) -> bool {
    key.is_some_and(|k| k.ed25519.len() == 32)
}

fn mirrors_match(entry: &DeviceEntry, cert: &DeviceCertBody) -> bool {
    entry.device_id == cert.device_id
        && entry.public_key == cert.public_key
        && entry.kind == cert.kind
        && entry.added_at == cert.added_at
        && entry.added_by == cert.added_by
        && entry.web_origin == cert.web_origin
        && key_ok(cert.public_key.as_ref())
}

// ---- 2. Identity -------------------------------------------------------------------

fn identity(body: &RosterBody) -> Result<Vec<u8>, RosterError> {
    let first = match body.rotations.first() {
        None => body
            .root_public_key
            .as_ref()
            .map(|k| k.ed25519.clone())
            .unwrap_or_default(),
        Some(rotation) => {
            let r = RootRotationBody::decode(rotation.body.as_slice())
                .map_err(|_| reject(Step::Chain, "undecodable"))?;
            r.from.map(|k| k.ed25519).unwrap_or_default()
        }
    };
    if blake3(&first) == body.identity_id {
        Ok(first)
    } else {
        Err(reject(Step::Identity, "mismatch"))
    }
}

// ---- 3. Rotation chain -------------------------------------------------------------

fn chain(body: &RosterBody, first: &[u8]) -> Result<Vec<Vec<u8>>, RosterError> {
    let mut roots = vec![first.to_vec()];
    let mut prev = first.to_vec();
    for rotation in &body.rotations {
        let r = RootRotationBody::decode(rotation.body.as_slice())
            .map_err(|_| reject(Step::Chain, "undecodable"))?;
        let from = r.from.map(|k| k.ed25519).unwrap_or_default();
        let to = r.to.map(|k| k.ed25519).unwrap_or_default();
        if from != prev {
            return Err(reject(Step::Chain, "broken"));
        }
        if r.identity_id != body.identity_id {
            return Err(reject(Step::Chain, "identity"));
        }
        let sig = rotation
            .signature
            .as_ref()
            .map(|s| s.signature.as_slice())
            .unwrap_or_default();
        if !verify(&rotation.body, sig, &from) {
            return Err(reject(Step::Chain, "signature"));
        }
        roots.push(to.clone());
        prev = to;
    }
    let current = body
        .root_public_key
        .as_ref()
        .map(|k| k.ed25519.clone())
        .unwrap_or_default();
    if prev != current {
        return Err(reject(Step::Chain, "head_mismatch"));
    }
    Ok(roots)
}

// ---- 4. Device certificates ---------------------------------------------------------

fn certs(
    body: &RosterBody,
    devices: &BTreeMap<Vec<u8>, DeviceInfo>,
    revoked: &BTreeMap<Vec<u8>, RevokedInfo>,
    roots: &[Vec<u8>],
    now_nanos: i128,
) -> Result<(), RosterError> {
    let skew_limit = now_nanos + CLOCK_SKEW_NANOS;
    let mut ordered: Vec<(&Vec<u8>, &DeviceInfo)> = devices.iter().collect();
    ordered.sort_by_key(|(_, d)| d.added_at);

    for (_, info) in ordered {
        let entry = body
            .devices
            .iter()
            .find(|e| e.device_id == info.cert.device_id)
            .ok_or_else(|| reject(Step::Certs, "signer"))?;
        if info.cert.identity_id != body.identity_id {
            return Err(reject(Step::Certs, "identity"));
        }
        if info.added_at > skew_limit {
            return Err(reject(Step::Certs, "future_dated"));
        }
        let Some(auth) = entry.authorization.as_ref() else {
            return Err(reject(Step::Certs, "signer"));
        };
        if !cert_signer_ok(auth, info, devices, revoked, roots) {
            return Err(reject(Step::Certs, "signer"));
        }
        let key = signer_key(auth, devices, revoked);
        if !verify(&entry.cert_body, &auth.signature, &key) {
            return Err(reject(Step::Certs, "signature"));
        }
    }
    Ok(())
}

fn cert_signer_ok(
    auth: &Signature,
    cert: &DeviceInfo,
    devices: &BTreeMap<Vec<u8>, DeviceInfo>,
    revoked: &BTreeMap<Vec<u8>, RevokedInfo>,
    roots: &[Vec<u8>],
) -> bool {
    match SignerKind::try_from(auth.signer_kind) {
        Ok(SignerKind::Root) => roots.contains(&auth.signer_id),
        Ok(SignerKind::Device) => {
            if auth.signer_id == cert.cert.device_id {
                return false;
            }
            if let Some(d) = devices.get(&auth.signer_id) {
                return d.added_at < cert.added_at;
            }
            if let Some(r) = revoked.get(&auth.signer_id) {
                return r.added_at < cert.added_at && r.revoked_at > cert.added_at;
            }
            false
        }
        _ => false,
    }
}

// ---- 5. Revocations ----------------------------------------------------------------

fn revocations(
    body: &RosterBody,
    revoked: &BTreeMap<Vec<u8>, RevokedInfo>,
    devices: &BTreeMap<Vec<u8>, DeviceInfo>,
    roots: &[Vec<u8>],
) -> Result<(), RosterError> {
    for entry in &body.revoked {
        let rb = RevocationBody::decode(entry.body.as_slice())
            .map_err(|_| reject(Step::Parse, "undecodable"))?;
        if rb.identity_id != body.identity_id {
            return Err(reject(Step::Revocations, "identity"));
        }
        let Some(sig) = entry.signature.as_ref() else {
            return Err(reject(Step::Revocations, "signer"));
        };
        let ok = match SignerKind::try_from(sig.signer_kind) {
            Ok(SignerKind::Root) => roots.contains(&sig.signer_id),
            Ok(SignerKind::Device) => {
                sig.signer_id != rb.device_id && devices.contains_key(&sig.signer_id)
            }
            _ => false,
        };
        if !ok {
            return Err(reject(Step::Revocations, "signer"));
        }
        let key = signer_key(sig, devices, &BTreeMap::new());
        if !verify(&entry.body, &sig.signature, &key) {
            return Err(reject(Step::Revocations, "signature"));
        }
        let _ = revoked;
    }
    Ok(())
}

// ---- 6. Roster signature ------------------------------------------------------------

fn signature(
    bytes: &[u8],
    sigs: &[Signature],
    body: &RosterBody,
    devices: &BTreeMap<Vec<u8>, DeviceInfo>,
) -> Result<bool, RosterError> {
    let current_root = body
        .root_public_key
        .as_ref()
        .map(|k| k.ed25519.clone())
        .unwrap_or_default();
    let valid: Vec<&Signature> = sigs
        .iter()
        .filter(|sig| match SignerKind::try_from(sig.signer_kind) {
            Ok(SignerKind::Root) => {
                sig.signer_id == current_root && verify(bytes, &sig.signature, &current_root)
            }
            Ok(SignerKind::Device) => devices
                .get(&sig.signer_id)
                .is_some_and(|d| verify(bytes, &sig.signature, &d.public_key)),
            _ => false,
        })
        .collect();
    if valid.is_empty() {
        return Err(reject(Step::Signature, "none_valid"));
    }
    Ok(valid
        .iter()
        .any(|s| s.signer_kind == SignerKind::Root as i32))
}

// ---- 7. Monotonicity ---------------------------------------------------------------

fn monotonic(
    body: &RosterBody,
    devices: &BTreeMap<Vec<u8>, DeviceInfo>,
    revoked: &BTreeMap<Vec<u8>, RevokedInfo>,
    held: Option<&Verified>,
) -> Result<Vec<Vec<u8>>, RosterError> {
    let Some(held) = held else {
        return Ok(revoked.keys().cloned().collect());
    };
    if body.identity_id != held.identity_id {
        return Err(reject(Step::Monotonic, "identity"));
    }
    if body.version <= held.version {
        return Err(reject(Step::Monotonic, "version"));
    }
    if !held
        .devices
        .keys()
        .all(|id| devices.contains_key(id) || revoked.contains_key(id))
    {
        return Err(reject(Step::Monotonic, "device_dropped"));
    }
    if !held.revoked.keys().all(|id| revoked.contains_key(id)) {
        return Err(reject(Step::Monotonic, "revocation_dropped"));
    }
    let held_rotations: Vec<&Vec<u8>> = held.body.rotations.iter().map(|r| &r.body).collect();
    let rotations: Vec<&Vec<u8>> = body.rotations.iter().map(|r| &r.body).collect();
    if !rotations.starts_with(&held_rotations) {
        return Err(reject(Step::Monotonic, "rotation_prefix"));
    }
    Ok(revoked
        .keys()
        .filter(|id| !held.revoked.contains_key(*id))
        .cloned()
        .collect())
}

fn signer_key(
    sig: &Signature,
    devices: &BTreeMap<Vec<u8>, DeviceInfo>,
    revoked: &BTreeMap<Vec<u8>, RevokedInfo>,
) -> Vec<u8> {
    match SignerKind::try_from(sig.signer_kind) {
        Ok(SignerKind::Root) => sig.signer_id.clone(),
        Ok(SignerKind::Device) => devices
            .get(&sig.signer_id)
            .map(|d| d.public_key.clone())
            .or_else(|| revoked.get(&sig.signer_id).map(|r| r.public_key.clone()))
            .unwrap_or_else(|| vec![0; 32]),
        _ => vec![0; 32],
    }
}

// ---- Building -------------------------------------------------------------------------

/// Who signs an entry.
#[derive(Debug, Clone, Copy)]
pub enum Signer<'a> {
    /// The root key.
    Root(&'a SigningKey),
    /// A device: its id and key.
    Device(&'a [u8], &'a SigningKey),
}

impl Signer<'_> {
    fn signature(&self, body: &[u8]) -> Signature {
        match self {
            Self::Root(key) => root_signature(key, body),
            Self::Device(id, key) => device_signature(id, key, body),
        }
    }

    fn added_by(&self) -> Vec<u8> {
        match self {
            Self::Root(_) => Vec::new(),
            Self::Device(id, _) => id.to_vec(),
        }
    }
}

/// A device to certify.
#[derive(Debug, Clone)]
pub struct NewDevice {
    /// Its id (a client-minted UUID, 16 bytes).
    pub device_id: Vec<u8>,
    /// Its Ed25519 public key.
    pub public_key: Vec<u8>,
    /// Its kind.
    pub kind: DeviceKind,
    /// Origin for WEB devices, else empty.
    pub web_origin: String,
    /// Human label, not signed.
    pub label: String,
    /// When it is being added.
    pub added_at: prost_types::Timestamp,
}

/// Builds a device certificate entry.
#[must_use]
pub fn device_entry(identity_id: &[u8], device: &NewDevice, signer: Signer<'_>) -> DeviceEntry {
    let cert = DeviceCertBody {
        identity_id: identity_id.to_vec(),
        device_id: device.device_id.clone(),
        public_key: Some(PublicKey {
            ed25519: device.public_key.clone(),
        }),
        kind: device.kind as i32,
        added_at: Some(device.added_at),
        added_by: signer.added_by(),
        web_origin: device.web_origin.clone(),
    };
    let cert_body = cert.encode_to_vec();
    let authorization = Some(signer.signature(&cert_body));
    DeviceEntry {
        cert_body,
        authorization,
        device_id: cert.device_id,
        public_key: cert.public_key,
        kind: cert.kind,
        added_at: cert.added_at,
        added_by: cert.added_by,
        web_origin: cert.web_origin,
        label: device.label.clone(),
    }
}

/// Builds a revocation entry for a device, carrying its key and added time.
#[must_use]
pub fn revoked_entry(
    identity_id: &[u8],
    device_id: &[u8],
    device_public_key: &[u8],
    added_at: prost_types::Timestamp,
    revoked_at: prost_types::Timestamp,
    signer: Signer<'_>,
) -> RevokedEntry {
    let body = RevocationBody {
        identity_id: identity_id.to_vec(),
        device_id: device_id.to_vec(),
        revoked_at: Some(revoked_at),
        revoked_by: signer.added_by(),
        public_key: Some(PublicKey {
            ed25519: device_public_key.to_vec(),
        }),
        added_at: Some(added_at),
    };
    let bytes = body.encode_to_vec();
    RevokedEntry {
        signature: Some(signer.signature(&bytes)),
        body: bytes,
        device_id: body.device_id,
        revoked_at: body.revoked_at,
        revoked_by: body.revoked_by,
    }
}

/// Builds a root rotation from `old` to `new_public_key`, signed by `old`.
#[must_use]
pub fn rotation(
    identity_id: &[u8],
    old: &SigningKey,
    new_public_key: &[u8],
    rotated_at: prost_types::Timestamp,
) -> RootRotation {
    let body = RootRotationBody {
        identity_id: identity_id.to_vec(),
        from: Some(PublicKey {
            ed25519: old.verifying_key().to_bytes().to_vec(),
        }),
        to: Some(PublicKey {
            ed25519: new_public_key.to_vec(),
        }),
        rotated_at: Some(rotated_at),
    };
    let bytes = body.encode_to_vec();
    RootRotation {
        signature: Some(root_signature(old, &bytes)),
        body: bytes,
    }
}

/// Assembles a roster body. `rotations` oldest first; `root_public_key` is
/// the current root.
#[must_use]
pub fn roster_body(
    identity_id: &[u8],
    version: u64,
    root_public_key: &[u8],
    rotations: Vec<RootRotation>,
    devices: Vec<DeviceEntry>,
    revoked: Vec<RevokedEntry>,
    issued_at: prost_types::Timestamp,
) -> RosterBody {
    RosterBody {
        identity_id: identity_id.to_vec(),
        version,
        root_public_key: Some(PublicKey {
            ed25519: root_public_key.to_vec(),
        }),
        rotations,
        devices,
        revoked,
        issued_at: Some(issued_at),
    }
}

// ---- Merging (spec section 5) ---------------------------------------------------------

/// Merges two forks of one identity's roster into version
/// `max(versions) + 1`, signed by `signer` (spec section 5): devices are
/// the union of both forks minus any device revoked in either,
/// revocations the union, and rotations the longer chain, which must have
/// the shorter as a prefix. Every entry keeps the signature it was carried
/// with, so the body verifies as a successor of both forks.
///
/// A device certified by a signer at or after that signer's revocation in
/// the other fork could not verify (the certificate rule of step 4), and
/// monotonicity forbids dropping it, so the merge revokes it, signed by
/// `signer`: a revoked device's later additions do not count.
///
/// # Errors
/// `Monotonic`/`identity` for forks of different identities;
/// `Chain`/`rotation_fork` when neither chain is a prefix of the other,
/// which no merge can reconcile.
pub fn merge(
    a: &Verified,
    b: &Verified,
    signer: Signer<'_>,
    issued_at: prost_types::Timestamp,
) -> Result<SignedRoster, RosterError> {
    if a.identity_id != b.identity_id {
        return Err(reject(Step::Monotonic, "identity"));
    }
    let (longer, shorter) = if a.body.rotations.len() >= b.body.rotations.len() {
        (a, b)
    } else {
        (b, a)
    };
    let longer_chain: Vec<&Vec<u8>> = longer.body.rotations.iter().map(|r| &r.body).collect();
    let shorter_chain: Vec<&Vec<u8>> = shorter.body.rotations.iter().map(|r| &r.body).collect();
    if !longer_chain.starts_with(&shorter_chain) {
        return Err(reject(Step::Chain, "rotation_fork"));
    }

    let mut revoked: BTreeMap<Vec<u8>, RevokedEntry> = BTreeMap::new();
    for entry in a.body.revoked.iter().chain(b.body.revoked.iter()) {
        revoked
            .entry(entry.device_id.clone())
            .or_insert_with(|| entry.clone());
    }
    let revoked_at = |id: &[u8]| {
        a.revoked
            .get(id)
            .or_else(|| b.revoked.get(id))
            .map(|r| r.revoked_at)
    };

    let mut devices: BTreeMap<Vec<u8>, DeviceEntry> = BTreeMap::new();
    let mut orphans: Vec<DeviceEntry> = Vec::new();
    for entry in a.body.devices.iter().chain(b.body.devices.iter()) {
        if revoked.contains_key(&entry.device_id) || devices.contains_key(&entry.device_id) {
            continue;
        }
        let added_at = ts_nanos(entry.added_at.as_ref());
        let signer_gone = !entry.added_by.is_empty()
            && revoked_at(&entry.added_by).is_some_and(|at| at <= added_at);
        if signer_gone {
            orphans.push(entry.clone());
        } else {
            devices.insert(entry.device_id.clone(), entry.clone());
        }
    }
    for orphan in orphans {
        let public_key = orphan
            .public_key
            .as_ref()
            .map(|k| k.ed25519.clone())
            .unwrap_or_default();
        revoked.insert(
            orphan.device_id.clone(),
            revoked_entry(
                &a.identity_id,
                &orphan.device_id,
                &public_key,
                orphan.added_at.unwrap_or_default(),
                issued_at,
                signer,
            ),
        );
    }

    let body = roster_body(
        &a.identity_id,
        a.version.max(b.version) + 1,
        &longer.root_public_key,
        longer.body.rotations.clone(),
        devices.into_values().collect(),
        revoked.into_values().collect(),
        issued_at,
    );
    Ok(sign_roster(&body, signer))
}

/// Serializes and signs a roster body.
#[must_use]
pub fn sign_roster(body: &RosterBody, signer: Signer<'_>) -> SignedRoster {
    let bytes = body.encode_to_vec();
    SignedRoster {
        signatures: vec![signer.signature(&bytes)],
        body: bytes,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::identity::keys::generate_signing_key;

    fn ts(seconds: i64) -> prost_types::Timestamp {
        prost_types::Timestamp { seconds, nanos: 0 }
    }

    #[test]
    fn a_built_roster_verifies_and_a_device_can_add_another() {
        let root = generate_signing_key().unwrap();
        let identity_id = blake3(&root.verifying_key().to_bytes());
        let phone = generate_signing_key().unwrap();
        let phone_id = vec![1u8; 16];
        let now = 1_700_000_000i64;

        let phone_entry = device_entry(
            &identity_id,
            &NewDevice {
                device_id: phone_id.clone(),
                public_key: phone.verifying_key().to_bytes().to_vec(),
                kind: DeviceKind::Native,
                web_origin: String::new(),
                label: "phone".into(),
                added_at: ts(now - 100),
            },
            Signer::Root(&root),
        );
        let body = RosterBody {
            identity_id: identity_id.clone(),
            version: 1,
            root_public_key: Some(PublicKey {
                ed25519: root.verifying_key().to_bytes().to_vec(),
            }),
            devices: vec![phone_entry.clone()],
            ..Default::default()
        };
        let v1 = sign_roster(&body, Signer::Root(&root));
        let held = verify_roster(&v1, None, i128::from(now) * 1_000_000_000).unwrap();
        assert!(held.root_signed);
        assert_eq!(held.devices.len(), 1);

        let laptop = generate_signing_key().unwrap();
        let laptop_id = vec![2u8; 16];
        let laptop_entry = device_entry(
            &identity_id,
            &NewDevice {
                device_id: laptop_id,
                public_key: laptop.verifying_key().to_bytes().to_vec(),
                kind: DeviceKind::Native,
                web_origin: String::new(),
                label: "laptop".into(),
                added_at: ts(now - 50),
            },
            Signer::Device(&phone_id, &phone),
        );
        let body2 = RosterBody {
            version: 2,
            devices: vec![phone_entry, laptop_entry],
            ..body
        };
        let v2 = sign_roster(&body2, Signer::Device(&phone_id, &phone));
        let verified = verify_roster(&v2, Some(&held), i128::from(now) * 1_000_000_000).unwrap();
        assert!(!verified.root_signed);
        assert_eq!(verified.devices.len(), 2);
        assert!(verified.new_revocations.is_empty());
    }

    /// An identity with two root-signed devices, as `Verified` at version 1.
    struct Two {
        root: SigningKey,
        identity_id: Vec<u8>,
        phone: (Vec<u8>, SigningKey),
        laptop: (Vec<u8>, SigningKey),
        v1: Verified,
        now: i64,
    }

    fn nanos(seconds: i64) -> i128 {
        i128::from(seconds) * 1_000_000_000
    }

    fn new_device(id: u8, key: &SigningKey, label: &str, added_at: i64) -> NewDevice {
        NewDevice {
            device_id: vec![id; 16],
            public_key: key.verifying_key().to_bytes().to_vec(),
            kind: DeviceKind::Native,
            web_origin: String::new(),
            label: label.into(),
            added_at: ts(added_at),
        }
    }

    fn two() -> Two {
        let root = generate_signing_key().unwrap();
        let identity_id = blake3(&root.verifying_key().to_bytes());
        let phone = generate_signing_key().unwrap();
        let laptop = generate_signing_key().unwrap();
        let now = 1_700_000_000i64;
        let body = roster_body(
            &identity_id,
            1,
            &root.verifying_key().to_bytes(),
            vec![],
            vec![
                device_entry(
                    &identity_id,
                    &new_device(1, &phone, "phone", now - 100),
                    Signer::Root(&root),
                ),
                device_entry(
                    &identity_id,
                    &new_device(2, &laptop, "laptop", now - 100),
                    Signer::Root(&root),
                ),
            ],
            vec![],
            ts(now - 100),
        );
        let v1 = verify_roster(&sign_roster(&body, Signer::Root(&root)), None, nanos(now)).unwrap();
        Two {
            root,
            identity_id,
            phone: (vec![1u8; 16], phone),
            laptop: (vec![2u8; 16], laptop),
            v1,
            now,
        }
    }

    /// `signer` adds `device` on top of `base`, as version `base + 1`.
    fn fork_adding(
        t: &Two,
        base: &Verified,
        signer: &(Vec<u8>, SigningKey),
        device: NewDevice,
    ) -> Verified {
        let mut devices = base.body.devices.clone();
        devices.push(device_entry(
            &t.identity_id,
            &device,
            Signer::Device(&signer.0, &signer.1),
        ));
        let body = roster_body(
            &t.identity_id,
            base.version + 1,
            &base.root_public_key,
            base.body.rotations.clone(),
            devices,
            base.body.revoked.clone(),
            ts(t.now),
        );
        let signed = sign_roster(&body, Signer::Device(&signer.0, &signer.1));
        verify_roster(&signed, Some(base), nanos(t.now)).unwrap()
    }

    /// `signer` revokes `target` on top of `base`, as version `base + 1`.
    fn fork_revoking(
        t: &Two,
        base: &Verified,
        signer: &(Vec<u8>, SigningKey),
        target: &[u8],
        at: i64,
    ) -> Verified {
        let info = base.devices.get(target).unwrap();
        let devices: Vec<DeviceEntry> = base
            .body
            .devices
            .iter()
            .filter(|d| d.device_id != target)
            .cloned()
            .collect();
        let mut revoked = base.body.revoked.clone();
        revoked.push(revoked_entry(
            &t.identity_id,
            target,
            &info.public_key,
            info.cert.added_at.unwrap(),
            ts(at),
            Signer::Device(&signer.0, &signer.1),
        ));
        let body = roster_body(
            &t.identity_id,
            base.version + 1,
            &base.root_public_key,
            base.body.rotations.clone(),
            devices,
            revoked,
            ts(t.now),
        );
        let signed = sign_roster(&body, Signer::Device(&signer.0, &signer.1));
        verify_roster(&signed, Some(base), nanos(t.now)).unwrap()
    }

    #[test]
    fn two_forks_adding_a_device_each_merge_with_both_devices_and_verify_as_a_successor_of_either()
    {
        let t = two();
        let tablet = generate_signing_key().unwrap();
        let watch = generate_signing_key().unwrap();
        let a = fork_adding(
            &t,
            &t.v1,
            &t.phone,
            new_device(3, &tablet, "tablet", t.now - 10),
        );
        let b = fork_adding(
            &t,
            &t.v1,
            &t.laptop,
            new_device(4, &watch, "watch", t.now - 10),
        );
        assert_eq!((a.version, b.version), (2, 2));
        assert_ne!(a.signed.body, b.signed.body);

        let signed = merge(&a, &b, Signer::Device(&t.phone.0, &t.phone.1), ts(t.now)).unwrap();
        let from_a = verify_roster(&signed, Some(&a), nanos(t.now)).unwrap();
        let from_b = verify_roster(&signed, Some(&b), nanos(t.now)).unwrap();
        assert_eq!(from_a.version, 3);
        assert_eq!(from_a.devices.len(), 4);
        assert!(from_a.devices.contains_key(&vec![3u8; 16]));
        assert!(from_a.devices.contains_key(&vec![4u8; 16]));
        assert!(from_b.new_revocations.is_empty());
        assert!(!from_a.root_signed);
        // Idempotent in content: merging the same forks again gives the same body.
        let again = merge(&b, &a, Signer::Device(&t.phone.0, &t.phone.1), ts(t.now)).unwrap();
        assert_eq!(
            RosterBody::decode(again.body.as_slice())
                .unwrap()
                .devices
                .len(),
            4
        );
    }

    #[test]
    fn a_revocation_in_one_fork_wins_and_takes_the_signers_later_additions_with_it() {
        let t = two();
        let watch = generate_signing_key().unwrap();
        let earlier = generate_signing_key().unwrap();
        // The laptop adds a watch after the moment the phone revokes it, and
        // an earlier device before that moment.
        let b1 = fork_adding(
            &t,
            &t.v1,
            &t.laptop,
            new_device(5, &earlier, "earlier", t.now - 60),
        );
        let b = fork_adding(
            &t,
            &b1,
            &t.laptop,
            new_device(4, &watch, "watch", t.now - 10),
        );
        let a = fork_revoking(&t, &t.v1, &t.phone, &t.laptop.0, t.now - 30);

        let signed = merge(&a, &b, Signer::Device(&t.phone.0, &t.phone.1), ts(t.now)).unwrap();
        let verified = verify_roster(&signed, Some(&a), nanos(t.now)).unwrap();
        let from_b = verify_roster(&signed, Some(&b), nanos(t.now)).unwrap();
        assert_eq!(verified.version, 4);
        assert!(verified.revoked.contains_key(&t.laptop.0));
        assert!(
            verified.devices.contains_key(&vec![5u8; 16]),
            "added before the revocation"
        );
        assert!(
            verified.revoked.contains_key(&vec![4u8; 16]),
            "added after it: revoked by the merge"
        );
        assert!(!verified.devices.contains_key(&t.laptop.0));
        // The instance holding the fork with the watch learns two revocations.
        assert_eq!(from_b.new_revocations.len(), 2);
    }

    #[test]
    fn forks_with_diverging_rotation_chains_are_irreconcilable() {
        let t = two();
        let new_a = generate_signing_key().unwrap();
        let new_b = generate_signing_key().unwrap();
        let rotated = |new_root: &SigningKey| {
            let body = roster_body(
                &t.identity_id,
                2,
                &new_root.verifying_key().to_bytes(),
                vec![rotation(
                    &t.identity_id,
                    &t.root,
                    &new_root.verifying_key().to_bytes(),
                    ts(t.now - 5),
                )],
                t.v1.body.devices.clone(),
                vec![],
                ts(t.now),
            );
            verify_roster(
                &sign_roster(&body, Signer::Root(new_root)),
                Some(&t.v1),
                nanos(t.now),
            )
            .unwrap()
        };
        let a = rotated(&new_a);
        let b = rotated(&new_b);
        let err = merge(&a, &b, Signer::Device(&t.phone.0, &t.phone.1), ts(t.now)).unwrap_err();
        assert_eq!((err.step, err.reason), (Step::Chain, "rotation_fork"));

        // One fork rotated, the other only added: the chain is the rotated one.
        let tablet = generate_signing_key().unwrap();
        let c = fork_adding(
            &t,
            &t.v1,
            &t.phone,
            new_device(3, &tablet, "tablet", t.now - 10),
        );
        let signed = merge(&a, &c, Signer::Device(&t.phone.0, &t.phone.1), ts(t.now)).unwrap();
        let merged = RosterBody::decode(signed.body.as_slice()).unwrap();
        assert_eq!(merged.rotations.len(), 1);
        assert_eq!(
            merged.root_public_key.as_ref().unwrap().ed25519,
            new_a.verifying_key().to_bytes().to_vec()
        );
        assert_eq!(
            verify_roster(&signed, Some(&c), nanos(t.now))
                .unwrap()
                .devices
                .len(),
            3
        );
        verify_roster(&signed, Some(&a), nanos(t.now)).unwrap();
    }
}
