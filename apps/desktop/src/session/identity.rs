//! This device's identity (architecture 5) and the account's own settings: the roster and
//! its devices, linking this computer to an identity (identity spec 4.2), revoking a device
//! (4.4), the account's passkeys, the signed profile, the status this person chose, and the
//! magic-link sign-in (architecture 6.3).
//!
//! Not built yet: making a passkey, which needs the browser handoff; approving another
//! device's link request from here; the per-instance state of a revocation ("revoked on 1
//! of 2"), since this shell knows one instance.

use lumia_client::app::{
    PasskeySummary, ProfileSummary, RosterSummary, SnapshotSummary, StatusSummary,
};

use super::{Failure, Session, Update};

/// What a finished identity or account call reports.
#[derive(Debug, Clone)]
pub enum IdentityUpdate {
    /// Step 1 of linking: the request to show for a device that holds the identity to
    /// approve.
    LinkRequested(Result<String, Failure>),
    /// The code was claimed: this device is in the roster and signed in by identity proof.
    Linked(Result<RosterSummary, Failure>),
    /// The roster this device holds.
    Roster(Result<RosterSummary, Failure>),
    /// A device was revoked: the roster after it.
    Revoked(Result<RosterSummary, Failure>),
    /// The account's passkeys on this instance.
    Passkeys(Result<Vec<PasskeySummary>, Failure>),
    /// A passkey was removed; the list is asked for again.
    PasskeyRemoved(Result<(), Failure>),
    /// The signed profile the identity carries; `None` when none was ever set.
    Profile(Result<Option<ProfileSummary>, Failure>),
    /// A new profile version was signed and sent.
    ProfileSaved(Result<ProfileSummary, Failure>),
    /// The account's own handle and display name were changed on the instance
    /// (`PATCH /account`): its snapshot as it then is.
    AccountSaved(Result<SnapshotSummary, Failure>),
    /// The instance was asked to mail a sign-in link.
    MagicLinkSent(Result<(), Failure>),
    /// A magic link's token was completed into a session.
    MagicLinkSignedIn(Result<(), Failure>),
}

/// What this device knows of its identity and the account's sign-in keys.
#[derive(Debug, Default)]
pub struct IdentityState {
    /// The roster this device holds, once asked for.
    pub roster: Option<RosterSummary>,
    /// The account's passkeys, once asked for.
    pub passkeys: Vec<PasskeySummary>,
    /// The signed profile, once asked for and when one was set.
    pub profile: Option<ProfileSummary>,
}

impl Session {
    /// Whether this device holds an identity (which personal conversations need). Read
    /// from the stores when the instance is opened and when a link is claimed, not on every
    /// call: the views ask each frame.
    pub fn has_identity(&self) -> bool {
        self.holds_identity
    }

    /// Reads from the stores whether this device holds an identity.
    pub(super) fn check_identity(&mut self) {
        self.holds_identity = self.client().is_some_and(|app| app.has_identity());
    }

    // ---- Linking ------------------------------------------------------------------------

    /// Starts linking this computer to an identity; the request to show arrives as
    /// [`IdentityUpdate::LinkRequested`].
    pub fn request_link(&self, label: &str) {
        let Some(app) = self.client() else { return };
        let label = label.to_string();
        self.task_local(move || async move {
            let request = app.identity_link_request(&label).await;
            Update::Identity(IdentityUpdate::LinkRequested(
                request.map_err(Failure::from),
            ))
        });
    }

    /// Claims the code an approving device gave and signs in by identity proof; the answer
    /// is [`IdentityUpdate::Linked`].
    pub fn claim_link(&self, code: &str) {
        let Some(app) = self.client() else { return };
        let code = code.trim().to_string();
        self.task_local(move || async move {
            let linked = async {
                let roster = app.identity_claim(&code).await?;
                app.sign_in_identity().await?;
                Ok::<_, lumia_client::ClientError>(roster)
            };
            Update::Identity(IdentityUpdate::Linked(linked.await.map_err(Failure::from)))
        });
    }

    // ---- Devices ------------------------------------------------------------------------

    /// Asks for the roster this device holds ([`IdentityUpdate::Roster`]).
    pub fn load_roster(&self) {
        let Some(app) = self.client() else { return };
        if !app.has_identity() {
            return;
        }
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Roster(
                app.identity_show().await.map_err(Failure::from),
            ))
        });
    }

    /// Revokes another device of the identity, signed by this one with the account
    /// password as reauth (identity spec 4.4); the answer is [`IdentityUpdate::Revoked`].
    pub fn revoke_device(&self, device_id: &str, password: &str) {
        let Some(app) = self.client() else { return };
        let (device_id, password) = (device_id.to_string(), password.to_string());
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Revoked(
                app.identity_revoke(&device_id, &password)
                    .await
                    .map_err(Failure::from),
            ))
        });
    }

    // ---- Passkeys -----------------------------------------------------------------------

    /// Asks for the account's passkeys ([`IdentityUpdate::Passkeys`]).
    pub fn load_passkeys(&self) {
        let Some(app) = self.client() else { return };
        self.task(async move {
            Update::Identity(IdentityUpdate::Passkeys(
                app.passkeys().await.map_err(Failure::from),
            ))
        });
    }

    /// Removes one of the account's passkeys ([`IdentityUpdate::PasskeyRemoved`]).
    pub fn remove_passkey(&self, passkey_id: &str) {
        let Some(app) = self.client() else { return };
        let passkey_id = passkey_id.to_string();
        self.task(async move {
            Update::Identity(IdentityUpdate::PasskeyRemoved(
                app.remove_passkey(&passkey_id).await.map_err(Failure::from),
            ))
        });
    }

    // ---- Profile and status -------------------------------------------------------------

    /// Asks for the signed profile the identity carries ([`IdentityUpdate::Profile`]):
    /// this device's, else the one the instance holds from another device.
    pub fn load_profile(&self) {
        let Some(app) = self.client() else { return };
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Profile(
                app.identity_profile().await.map_err(Failure::from),
            ))
        });
    }

    /// Signs a new profile version with this device's key and sends it to the instance,
    /// keeping the avatar the held one names; the answer is
    /// [`IdentityUpdate::ProfileSaved`], and the snapshot follows so the name on screen does.
    pub fn save_profile(&self, display_name: &str, pronouns: &str, bio: &str) {
        let Some(app) = self.client() else { return };
        let (display_name, pronouns, bio) = (
            display_name.trim().to_string(),
            pronouns.trim().to_string(),
            bio.trim().to_string(),
        );
        self.task_local(move || async move {
            let avatar = app
                .identity_profile()
                .await
                .ok()
                .flatten()
                .map(|held| held.avatar_url)
                .unwrap_or_default();
            Update::Identity(IdentityUpdate::ProfileSaved(
                app.identity_set_profile(&display_name, &pronouns, &bio, &avatar)
                    .await
                    .map_err(Failure::from),
            ))
        });
    }

    /// Changes the account's handle on the instance, and its display name when one is
    /// given (the form leaves it out on a computer that holds the identity, where the
    /// signed profile carries the name). Any signed-in device may. The answer is
    /// [`IdentityUpdate::AccountSaved`].
    pub fn save_account(&self, handle: &str, display_name: Option<&str>) {
        let Some(app) = self.client() else { return };
        let (handle, display_name) = (handle.trim().to_string(), display_name.map(str::to_string));
        self.task(async move {
            Update::Identity(IdentityUpdate::AccountSaved(
                app.update_account(Some(&handle), display_name.as_deref())
                    .await
                    .map_err(Failure::from),
            ))
        });
    }

    /// The status this person chose (`ONLINE`, `AWAY` or `DO_NOT_DISTURB`) and their custom
    /// status text (protocol 6.7), from the local store.
    pub fn status(&self) -> Option<StatusSummary> {
        self.client()?.status().ok()
    }

    /// Chooses the status and the custom text: the core keeps it and sends it with the
    /// servers watched.
    ///
    /// # Errors
    /// When no instance is open, or the store refuses the write.
    pub fn set_status(&self, status: &str, custom_status: &str) -> Result<(), String> {
        let app = self.client().ok_or("No instance is open.")?;
        app.set_status(status, custom_status)
            .map_err(|error| error.to_string())
    }

    // ---- Magic link ---------------------------------------------------------------------

    /// Asks the instance to mail a sign-in link to `email` (architecture 6.3); the answer
    /// is [`IdentityUpdate::MagicLinkSent`].
    pub fn request_magic_link(&self, email: &str) {
        let Some(app) = self.client() else { return };
        let email = email.trim().to_string();
        self.task(async move {
            Update::Identity(IdentityUpdate::MagicLinkSent(
                app.request_magic_link(&email).await.map_err(Failure::from),
            ))
        });
    }

    /// Signs in with the token of a magic-link mail, pasted as the code or as the whole
    /// link; the answer is [`IdentityUpdate::MagicLinkSignedIn`].
    pub fn sign_in_magic_link(&self, token: &str) {
        let Some(app) = self.client() else { return };
        let token = token.trim().to_string();
        self.task(async move {
            Update::Identity(IdentityUpdate::MagicLinkSignedIn(
                app.sign_in_magic_link(&token).await.map_err(Failure::from),
            ))
        });
    }

    pub(super) fn apply_identity(&mut self, update: &IdentityUpdate) {
        match update {
            IdentityUpdate::Linked(Ok(roster)) => {
                self.identity.roster = Some(roster.clone());
                self.check_identity();
                self.signed_in = true;
                self.load();
            }
            IdentityUpdate::Roster(Ok(roster)) | IdentityUpdate::Revoked(Ok(roster)) => {
                self.identity.roster = Some(roster.clone());
            }
            IdentityUpdate::Passkeys(Ok(passkeys)) => self.identity.passkeys = passkeys.clone(),
            IdentityUpdate::PasskeyRemoved(Ok(())) => self.load_passkeys(),
            IdentityUpdate::Profile(Ok(profile)) => self.identity.profile = profile.clone(),
            IdentityUpdate::ProfileSaved(Ok(profile)) => {
                self.identity.profile = Some(profile.clone());
                // The account's name on this instance is the profile's now.
                self.load();
            }
            IdentityUpdate::MagicLinkSignedIn(Ok(())) => {
                self.signed_in = true;
                self.load();
            }
            IdentityUpdate::AccountSaved(Ok(snapshot)) => {
                // The lists that name this person were read before the change.
                let me = snapshot.account_id.as_str();
                let listed = self.members.values_mut().flatten();
                for person in listed.chain(self.people.values_mut()) {
                    if person.account_id == me {
                        person.handle.clone_from(&snapshot.handle);
                        person.display_name.clone_from(&snapshot.display_name);
                    }
                }
                self.snapshot = Some(snapshot.clone());
            }
            IdentityUpdate::LinkRequested(_)
            | IdentityUpdate::MagicLinkSent(_)
            | IdentityUpdate::Linked(Err(_))
            | IdentityUpdate::Roster(Err(_))
            | IdentityUpdate::Revoked(Err(_))
            | IdentityUpdate::Passkeys(Err(_))
            | IdentityUpdate::PasskeyRemoved(Err(_))
            | IdentityUpdate::Profile(Err(_))
            | IdentityUpdate::ProfileSaved(Err(_))
            | IdentityUpdate::AccountSaved(Err(_))
            | IdentityUpdate::MagicLinkSignedIn(Err(_)) => {}
        }
    }
}
