//! The identity flows a client runs (docs/specs/identity.md section 4),
//! over the core's builders and the API. State per instance: the device
//! key, backup key and link secret in the keystore; the identity id and
//! current roster in the store's meta area.

use base64::Engine;
use ed25519_dalek::SigningKey;
use lumia_core::identity::keys::{generate_signing_key, open, seal, signing_key, ts_nanos};
use lumia_core::identity::link::{seal_bundle, LinkRequester};
use lumia_core::identity::phrase::Phrase;
use lumia_core::identity::proof::build_proof;
use lumia_core::identity::roster::{
    device_entry, revoked_entry, roster_body, sign_roster, verify_roster, NewDevice, Signer,
    Verified,
};
use lumia_core::keystore::Keystore;
use lumia_core::proto::{
    ChallengePurpose, CreateLinkBundleRequest, DeviceKind, HostAccount, IdentityBackupBody,
    LinkBundleBody, LinkRequest, SignedRoster, TokenPair,
};
use lumia_core::store::Store;
use lumia_core::wire;
use prost::Message;

use crate::api::Api;
use crate::ClientError;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Everything the flows touch for one instance.
pub struct Device<'a, S: Store, K: Keystore> {
    /// The local store (identity id and roster live in its meta area).
    pub store: &'a mut S,
    /// The keystore (device key, backup key, link secret).
    pub keystore: &'a mut K,
    /// This device's id.
    pub device_id: Vec<u8>,
}

impl<S: Store, K: Keystore> std::fmt::Debug for Device<'_, S, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("device_id", &wire::display(&self.device_id))
            .finish_non_exhaustive()
    }
}

/// What `create` returns: the phrase to show once, and the identity id.
#[derive(Debug)]
pub struct Created {
    /// The 24 words. Shown once; never stored.
    pub phrase: String,
    /// The identity id, hex.
    pub identity_id: String,
}

impl<S: Store, K: Keystore> Device<'_, S, K> {
    /// Creates a new identity on this device (spec 4.1): a phrase, a root,
    /// this device's key, and roster version 1 signed by the root. The root
    /// private key is dropped at the end of this function.
    ///
    /// # Errors
    /// On keystore, store, or randomness failure.
    pub fn create(&mut self, label: &str) -> Result<Created, ClientError> {
        let phrase = Phrase::generate()?;
        let derived = phrase.derive();
        let identity_id = derived.identity_id();
        let device_key = self.device_key_or_generate()?;

        let entry = device_entry(
            &identity_id,
            &NewDevice {
                device_id: self.device_id.clone(),
                public_key: device_key.verifying_key().to_bytes().to_vec(),
                kind: DeviceKind::Native,
                web_origin: String::new(),
                label: label.to_string(),
                added_at: now(),
            },
            Signer::Root(&derived.root),
        );
        let body = roster_body(
            &identity_id,
            1,
            &derived.root.verifying_key().to_bytes(),
            vec![],
            vec![entry],
            vec![],
            now(),
        );
        let roster = sign_roster(&body, Signer::Root(&derived.root));

        self.keystore.put("backup_key", &derived.backup_key)?;
        self.save_roster(&identity_id, &roster)?;
        Ok(Created {
            phrase: phrase.words(),
            identity_id: hex::encode(identity_id),
        })
    }

    /// The verified current roster, or an error when this device has none.
    ///
    /// # Errors
    /// When no roster is stored or it does not verify.
    pub fn roster(&self) -> Result<Verified, ClientError> {
        let b64 = self.store.meta_get("roster")?.ok_or_else(|| {
            ClientError::State(
                "no identity on this device; run `identity create` or `identity claim`".into(),
            )
        })?;
        let bytes = B64
            .decode(b64)
            .map_err(|e| ClientError::State(e.to_string()))?;
        let signed = SignedRoster::decode(bytes.as_slice())?;
        Ok(verify_roster(&signed, None, ts_nanos(Some(&now())))?)
    }

    /// Proves this identity to `api`'s host for `purpose`.
    ///
    /// # Errors
    /// On API failure, a challenge for another host, or missing state.
    pub async fn prove(
        &self,
        api: &Api,
        purpose: ChallengePurpose,
    ) -> Result<lumia_core::proto::IdentityProof, ClientError> {
        let verified = self.roster()?;
        let key = self.device_key()?;
        let challenge = api.challenge(purpose).await?;
        Ok(build_proof(
            &challenge.challenge,
            &api.host(),
            purpose,
            &verified.identity_id,
            &self.device_id,
            &key,
            verified.signed,
        )?)
    }

    /// Creates an account on the instance by identity proof (spec 4.1 step 5).
    /// A stale roster is reconciled once (spec 5, "older roster").
    ///
    /// # Errors
    /// On API failure, or [`ClientError::DeviceRevoked`] after adopting a
    /// held roster that revokes this device.
    pub async fn create_account(
        &mut self,
        api: &Api,
        handle: &str,
    ) -> Result<TokenPair, ClientError> {
        let proof = self.prove(api, ChallengePurpose::CreateAccount).await?;
        match api.create_account_by_identity(handle, proof).await {
            Err(e) if held_in(&e).is_some() => {
                self.reconcile(&e)?;
                let proof = self.prove(api, ChallengePurpose::CreateAccount).await?;
                api.create_account_by_identity(handle, proof).await
            }
            other => other,
        }
    }

    /// Signs in by identity proof. A stale roster is reconciled once: the
    /// held version is adopted and, if this device is still in it, the
    /// sign-in is retried with it.
    ///
    /// # Errors
    /// On API failure (including `POLICY_DENIED` when the instance forbids
    /// it), or [`ClientError::DeviceRevoked`] after adopting a held roster
    /// that revokes this device.
    pub async fn sign_in(&mut self, api: &Api) -> Result<TokenPair, ClientError> {
        let proof = self.prove(api, ChallengePurpose::SignIn).await?;
        match api.sign_in_identity(proof).await {
            Err(e) if held_in(&e).is_some() => {
                self.reconcile(&e)?;
                let proof = self.prove(api, ChallengePurpose::SignIn).await?;
                api.sign_in_identity(proof).await
            }
            other => other,
        }
    }

    /// Binds the signed-in password account to this device's identity
    /// (`POST /identity/link`, challenge purpose `LINK_ACCOUNT`), under a
    /// fresh reauth by password.
    ///
    /// # Errors
    /// On API failure: a wrong password, or `CONFLICT` when the account is
    /// bound to another identity.
    pub async fn link_account(&mut self, api: &Api, password: &str) -> Result<(), ClientError> {
        let reauth = api.reauth_password(password).await?;
        let proof = self.prove(api, ChallengePurpose::LinkAccount).await?;
        api.link_identity(proof, &reauth.reauth_token).await
    }

    /// Adopts the roster an instance returned in `CONFLICT`'s `details.held`
    /// (spec 5, "older roster"). It must verify against the one held here
    /// and be newer.
    ///
    /// # Errors
    /// When the held roster is malformed or does not verify as a successor.
    pub fn adopt_held(&mut self, held_b64: &str) -> Result<Verified, ClientError> {
        let current = self.roster()?;
        let bytes = B64
            .decode(held_b64.trim())
            .map_err(|e| ClientError::State(format!("held roster: {e}")))?;
        let signed = SignedRoster::decode(bytes.as_slice())?;
        let verified = verify_roster(&signed, Some(&current), now_nanos())?;
        self.save_roster(&verified.identity_id, &signed)?;
        Ok(verified)
    }

    /// Adopts the held roster carried by a `CONFLICT`. `Ok` means this
    /// device is still a member and the caller may retry.
    fn reconcile(&mut self, err: &ClientError) -> Result<Verified, ClientError> {
        let held = held_in(err).ok_or_else(|| ClientError::State(err.to_string()))?;
        let verified = self.adopt_held(held)?;
        if verified.revoked.contains_key(&self.device_id)
            || !verified.devices.contains_key(&self.device_id)
        {
            return Err(ClientError::DeviceRevoked {
                roster_version: verified.version,
            });
        }
        Ok(verified)
    }

    /// Step 1 of adding this device (spec 4.2): the request to show as a
    /// QR, base64. The ephemeral secret is kept until `claim`.
    ///
    /// # Errors
    /// On keystore or randomness failure.
    pub fn link_request(&mut self, label: &str) -> Result<String, ClientError> {
        let key = self.device_key_or_generate()?;
        let requester = LinkRequester::new(
            self.device_id.clone(),
            key.verifying_key().to_bytes().to_vec(),
            DeviceKind::Native,
            label,
            "",
        )?;
        self.keystore
            .put("link_secret", &requester.secret_bytes())?;
        let request = requester.request.encode_to_vec();
        self.store.meta_set("link_request", &B64.encode(&request))?;
        Ok(B64.encode(request))
    }

    /// Steps 2 to 6 on the existing device: certify the requesting device,
    /// publish the new roster, upload the encrypted bundle. Returns the code.
    ///
    /// # Errors
    /// On a malformed request, API failure, or missing state.
    pub async fn approve(&mut self, api: &Api, request_b64: &str) -> Result<String, ClientError> {
        let bytes = B64
            .decode(request_b64.trim())
            .map_err(|e| ClientError::State(format!("bad request: {e}")))?;
        let request = LinkRequest::decode(bytes.as_slice())?;
        let kind = DeviceKind::try_from(request.kind).unwrap_or(DeviceKind::Native);

        // Build on the roster held here; if the instance holds a newer one,
        // adopt it and build again (spec 5, "older roster").
        let mut verified = self.roster()?;
        let mut reconciled = false;
        let roster = loop {
            let roster = self.addition_roster(&verified, &request, kind)?;
            match api.publish_roster(roster.clone()).await {
                Ok(()) => break roster,
                Err(e) if held_in(&e).is_some() && !reconciled => {
                    verified = self.reconcile(&e)?;
                    reconciled = true;
                }
                Err(e) => return Err(e),
            }
        };
        self.save_roster(&verified.identity_id, &roster)?;

        let backup_key = if kind == DeviceKind::Web {
            Vec::new()
        } else {
            self.keystore.get("backup_key")?.unwrap_or_default()
        };
        let bundle = LinkBundleBody {
            roster: Some(roster),
            backup_key,
            ..Default::default()
        };
        let sealed = seal_bundle(&request, bundle)?;
        let created = api
            .create_link_bundle(CreateLinkBundleRequest {
                ciphertext: sealed.ciphertext,
                sender_ephemeral_public_key: sealed.sender_ephemeral_public_key,
                nonce: sealed.nonce,
                device_id: request.device_id,
            })
            .await?;
        Ok(created.code)
    }

    /// Step 7 on the new device: fetch and open the bundle, adopt the roster.
    ///
    /// # Errors
    /// On API failure, decryption failure, or a roster that does not verify.
    pub async fn claim(&mut self, api: &Api, code: &str) -> Result<Verified, ClientError> {
        let secret = self
            .keystore
            .get("link_secret")?
            .ok_or_else(|| ClientError::State("no link request on this device".into()))?;
        let request_b64 = self
            .store
            .meta_get("link_request")?
            .ok_or_else(|| ClientError::State("no link request on this device".into()))?;
        let request = LinkRequest::decode(
            B64.decode(request_b64)
                .map_err(|e| ClientError::State(e.to_string()))?
                .as_slice(),
        )?;
        let requester = LinkRequester::from_parts(&secret, request)?;

        let bundle = api
            .get_link_bundle(code, &wire::display(&self.device_id))
            .await?;
        let body = requester.open_bundle(
            &bundle.sender_ephemeral_public_key,
            &bundle.nonce,
            &bundle.ciphertext,
        )?;
        let roster = body
            .roster
            .ok_or_else(|| ClientError::State("bundle has no roster".into()))?;
        let verified = verify_roster(&roster, None, ts_nanos(Some(&now())))?;
        if !verified.devices.contains_key(&self.device_id) {
            return Err(ClientError::State(
                "the roster does not include this device".into(),
            ));
        }
        if !body.backup_key.is_empty() {
            self.keystore.put("backup_key", &body.backup_key)?;
        }
        self.save_roster(&verified.identity_id, &roster)?;
        self.keystore.delete("link_secret")?;
        Ok(verified)
    }

    /// Writes the identity backup to `api`'s instance (spec 8): the roster,
    /// the host accounts this device knows, and a version above the last
    /// one written, sealed under the backup key.
    ///
    /// # Errors
    /// Without a backup key (web devices), or on API failure.
    pub async fn write_backup(
        &mut self,
        api: &Api,
        accounts: Vec<HostAccount>,
    ) -> Result<u64, ClientError> {
        let key = self
            .keystore
            .get("backup_key")?
            .ok_or_else(|| ClientError::State("this device holds no backup key".into()))?;
        let verified = self.roster()?;
        let version = self
            .store
            .meta_get("backup_version")?
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            + 1;
        let body = IdentityBackupBody {
            version,
            roster: Some(verified.signed),
            accounts,
            written_at: Some(now()),
            ..Default::default()
        };
        let (nonce, ciphertext) = seal(&key, &body.encode_to_vec())?;
        api.put_identity_backup(version, ciphertext, nonce).await?;
        self.store
            .meta_set("backup_version", &version.to_string())?;
        Ok(version)
    }

    /// Reads and opens the identity backup held by `api`'s instance.
    ///
    /// # Errors
    /// Without a backup key, when none is held (`NOT_FOUND`), or when it
    /// does not open under this device's backup key.
    pub async fn read_backup(&self, api: &Api) -> Result<IdentityBackupBody, ClientError> {
        let key = self
            .keystore
            .get("backup_key")?
            .ok_or_else(|| ClientError::State("this device holds no backup key".into()))?;
        let held = api.get_identity_backup().await?;
        let plain = open(&key, &held.nonce, &held.ciphertext)?;
        Ok(IdentityBackupBody::decode(plain.as_slice())?)
    }

    /// Recovery with no device left (spec 4.3): from the phrase, a fresh
    /// device key and a root-signed roster that revokes every device the
    /// instance holds and adds this one, published by identity-proof
    /// sign-in. The held version is learned from the `CONFLICT` a
    /// provisional roster provokes; without a held roster the provisional
    /// one stands. Returns the token pair for this instance.
    ///
    /// # Errors
    /// When the phrase does not match the identity the instance holds, or
    /// on API failure.
    pub async fn recover(
        &mut self,
        api: &Api,
        phrase: &str,
        label: &str,
    ) -> Result<TokenPair, ClientError> {
        let derived = Phrase::parse(phrase)?.derive();
        let identity_id = derived.identity_id();
        let root_public = derived.root.verifying_key().to_bytes().to_vec();
        let device_key = self.device_key_or_generate()?;
        let new_device = NewDevice {
            device_id: self.device_id.clone(),
            public_key: device_key.verifying_key().to_bytes().to_vec(),
            kind: DeviceKind::Native,
            web_origin: String::new(),
            label: label.to_string(),
            added_at: now(),
        };
        let entry = device_entry(&identity_id, &new_device, Signer::Root(&derived.root));

        // Provisional: version 1 with this device only.
        let body = roster_body(
            &identity_id,
            1,
            &root_public,
            vec![],
            vec![entry.clone()],
            vec![],
            now(),
        );
        self.keystore.put("backup_key", &derived.backup_key)?;
        self.save_roster(
            &identity_id,
            &sign_roster(&body, Signer::Root(&derived.root)),
        )?;

        let proof = self.prove(api, ChallengePurpose::SignIn).await?;
        let held = match api.sign_in_identity(proof).await {
            Ok(tokens) => return Ok(tokens),
            Err(e) => match held_in(&e) {
                Some(held) => held.to_string(),
                None => return Err(e),
            },
        };
        let bytes = B64
            .decode(held.trim())
            .map_err(|e| ClientError::State(format!("held roster: {e}")))?;
        let held = verify_roster(&SignedRoster::decode(bytes.as_slice())?, None, now_nanos())?;
        if !held.roots.contains(&root_public) {
            return Err(ClientError::State(
                "that phrase is not this identity's root".into(),
            ));
        }

        // The real one: every held device revoked by the root, this one added.
        let mut revoked = held.body.revoked.clone();
        for (device_id, info) in &held.devices {
            revoked.push(revoked_entry(
                &identity_id,
                device_id,
                &info.public_key,
                info.cert.added_at.unwrap_or_default(),
                now(),
                Signer::Root(&derived.root),
            ));
        }
        let body = roster_body(
            &identity_id,
            held.version + 1,
            &held.root_public_key,
            held.body.rotations.clone(),
            vec![entry],
            revoked,
            now(),
        );
        self.save_roster(
            &identity_id,
            &sign_roster(&body, Signer::Root(&derived.root)),
        )?;
        let proof = self.prove(api, ChallengePurpose::SignIn).await?;
        api.sign_in_identity(proof).await
    }

    /// Revokes a device with the root, from the phrase (spec 4.3 and 4.4's
    /// root path). Publishes the root-signed roster, which bypasses reauth.
    ///
    /// # Errors
    /// When the phrase does not match this identity's root, the device is
    /// unknown, or the API refuses.
    pub async fn revoke_with_phrase(
        &mut self,
        api: &Api,
        phrase: &str,
        device_id: &[u8],
    ) -> Result<(), ClientError> {
        let derived = Phrase::parse(phrase)?.derive();
        let verified = self.roster()?;
        if derived.root.verifying_key().to_bytes().as_slice() != verified.root_public_key.as_slice()
        {
            return Err(ClientError::State(
                "that phrase is not this identity's root".into(),
            ));
        }
        let roster = revocation_roster(&verified, device_id, Signer::Root(&derived.root))?;
        api.publish_roster(roster.clone()).await?;
        self.save_roster(&verified.identity_id, &roster)
    }

    /// Revokes a device with this device's key (spec 4.3, the device path),
    /// which the instance accepts only with a fresh reauth: the account's
    /// password here. Goes to `POST /identity/devices/revoke`.
    ///
    /// # Errors
    /// On a wrong password, an unknown device, or `POLICY_DENIED` when the
    /// instance does not accept the reauth.
    pub async fn revoke_with_device(
        &mut self,
        api: &Api,
        password: &str,
        device_id: &[u8],
    ) -> Result<(), ClientError> {
        if device_id == self.device_id.as_slice() {
            return Err(ClientError::State(
                "a device cannot revoke itself; use the phrase or another device".into(),
            ));
        }
        let verified = self.roster()?;
        let my_key = self.device_key()?;
        let my_id = self.device_id.clone();
        let roster = revocation_roster(&verified, device_id, Signer::Device(&my_id, &my_key))?;
        let reauth = api.reauth_password(password).await?;
        api.revoke_device(roster.clone(), &reauth.reauth_token)
            .await?;
        self.save_roster(&verified.identity_id, &roster)
    }

    /// The next roster with the requesting device certified by this one.
    fn addition_roster(
        &self,
        verified: &Verified,
        request: &LinkRequest,
        kind: DeviceKind,
    ) -> Result<SignedRoster, ClientError> {
        let my_key = self.device_key()?;
        let new_key = request
            .device_public_key
            .as_ref()
            .map(|k| k.ed25519.clone())
            .unwrap_or_default();
        let entry = device_entry(
            &verified.identity_id,
            &NewDevice {
                device_id: request.device_id.clone(),
                public_key: new_key,
                kind,
                web_origin: request.web_origin.clone(),
                label: request.label.clone(),
                added_at: now(),
            },
            Signer::Device(&self.device_id, &my_key),
        );
        let mut devices = verified.body.devices.clone();
        devices.push(entry);
        let body = roster_body(
            &verified.identity_id,
            verified.version + 1,
            &verified.root_public_key,
            verified.body.rotations.clone(),
            devices,
            verified.body.revoked.clone(),
            now(),
        );
        Ok(sign_roster(&body, Signer::Device(&self.device_id, &my_key)))
    }

    fn device_key(&self) -> Result<SigningKey, ClientError> {
        let seed = self
            .keystore
            .get("device_key")?
            .ok_or_else(|| ClientError::State("no device key".into()))?;
        Ok(signing_key(&seed)?)
    }

    fn device_key_or_generate(&mut self) -> Result<SigningKey, ClientError> {
        match self.keystore.get("device_key")? {
            Some(seed) => Ok(signing_key(&seed)?),
            None => {
                let key = generate_signing_key()?;
                self.keystore.put("device_key", &key.to_bytes())?;
                Ok(key)
            }
        }
    }

    fn save_roster(
        &mut self,
        identity_id: &[u8],
        roster: &SignedRoster,
    ) -> Result<(), ClientError> {
        self.store
            .meta_set("identity_id", &hex::encode(identity_id))?;
        self.store
            .meta_set("roster", &B64.encode(roster.encode_to_vec()))?;
        Ok(())
    }
}

/// The next roster with `device_id` moved to `revoked`, signed by `signer`.
fn revocation_roster(
    verified: &Verified,
    device_id: &[u8],
    signer: Signer<'_>,
) -> Result<SignedRoster, ClientError> {
    let info = verified
        .devices
        .get(device_id)
        .ok_or_else(|| ClientError::State("unknown device".into()))?;
    let added_at = info.cert.added_at.unwrap_or_default();
    let entry = revoked_entry(
        &verified.identity_id,
        device_id,
        &info.public_key,
        added_at,
        now(),
        signer,
    );
    let devices: Vec<_> = verified
        .body
        .devices
        .iter()
        .filter(|d| d.device_id != device_id)
        .cloned()
        .collect();
    let mut revoked = verified.body.revoked.clone();
    revoked.push(entry);
    let body = roster_body(
        &verified.identity_id,
        verified.version + 1,
        &verified.root_public_key,
        verified.body.rotations.clone(),
        devices,
        revoked,
        now(),
    );
    Ok(sign_roster(&body, signer))
}

/// The held roster a `CONFLICT` carries, when it does.
#[must_use]
pub fn held_in(err: &ClientError) -> Option<&str> {
    match err {
        ClientError::Api { code, details, .. } if code == "CONFLICT" => {
            details.get("held").map(String::as_str)
        }
        _ => None,
    }
}

/// The wall clock as verification nanoseconds, for callers that verify a
/// roster outside this module.
pub fn now_nanos() -> i128 {
    ts_nanos(Some(&now()))
}

fn now() -> prost_types::Timestamp {
    crate::time::now_ts()
}
