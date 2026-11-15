//! This device's identity (architecture 5) and the account's own settings: setting up
//! private messages (architecture 5.4, identity spec 4.1, 4.3 and 4.7), the roster and its
//! devices, linking this computer to an identity (identity spec 4.2, and by a prompt on the
//! account's other devices, 4.2.1, in either role), revoking a device (4.4), the account's
//! passkeys, the signed profile, the status this person chose, and the magic-link sign-in
//! (architecture 6.3).
//!
//! Setting up: after every sign-in, and at a launch that resumes a session, the account is
//! set up on this computer silently ([`Session::check_setup`], the core's
//! `identity_setup`): an identity made here unless one is held, the account bound to it
//! under the reauth the sign-in carried. What that comes to is [`Setup`], which the
//! screens read; the phrase the core keeps until three of its words are confirmed is
//! [`IdentityState::phrase_kept`], which the reminders follow.
//!
//! Not built yet: making a passkey, which needs the browser handoff; the per-instance state of a revocation ("revoked on 1
//! of 2"), since this shell knows one instance; reauth by a mailed code or the instance's
//! OIDC provider (the core has both), so an account without a password can neither finish
//! a setup that needs reauth nor start over here.

use lumia_client::ClientError;
use lumia_client::app::{
    LinkProgress, LinkPrompt, PasskeySummary, ProfileSummary, RosterSummary, SnapshotSummary,
    StatusSummary,
};

use super::{Failure, Session, Update};

/// The label this computer's device carries in its roster, as the other desktop shells
/// send it.
pub const DEVICE_LABEL: &str = "desktop";

/// Where setting up private messages on this computer stands for the signed-in account
/// (architecture 5.4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Setup {
    /// Not asked yet, or asked and not answered.
    #[default]
    Checking,
    /// The account is bound to the identity this computer holds.
    Ready,
    /// The account has private messages on another device: this computer needs linking,
    /// restoring from the phrase, or, with neither, starting over.
    Elsewhere,
    /// The account has no identity yet, and binding it needs the person to confirm it is
    /// them: the sign-in carried no reauth (a launch, or a sign-in by identity proof).
    NeedsReauth,
    /// The instance could not be asked, or refused: what it said.
    Failed(String),
}

impl Setup {
    fn of(answer: Result<String, ClientError>) -> Self {
        match answer {
            Ok(_) => Self::Ready,
            Err(ClientError::AccountHasIdentity) => Self::Elsewhere,
            Err(ClientError::ReauthNeeded) => Self::NeedsReauth,
            Err(error) => Self::Failed(Failure::from(error).message),
        }
    }
}

/// What a finished identity or account call reports.
#[derive(Debug, Clone)]
pub enum IdentityUpdate {
    /// Step 1 of linking: the request to show for a device that holds the identity to
    /// approve.
    LinkRequested(Result<String, Failure>),
    /// The code was claimed: this device is in the roster and signed in by identity proof.
    /// Linking by a prompt ends here too, at its `linked` stage.
    Linked(Result<RosterSummary, Failure>),
    /// Linking by a prompt (identity spec 4.2.1), this computer as the new device: the
    /// session was opened, and every device of the account that holds the identity asks.
    LinkStarted(Result<LinkProgress, Failure>),
    /// Where that session stands now, short of `linked` (which is [`Self::Linked`]).
    LinkProgressed(Result<LinkProgress, Failure>),
    /// The session was given up.
    LinkCancelled,
    /// This computer, which holds the identity: the account's link sessions that prompt
    /// here, oldest first.
    Prompts(Result<Vec<LinkPrompt>, Failure>),
    /// A prompt was answered here ("Continue").
    PromptAnswered(Result<(), Failure>),
    /// A prompt was approved here: the new device is in the roster and has its code.
    PromptConfirmed(Result<(), Failure>),
    /// A prompt was declined here ("Not me", "They don’t match").
    PromptDeclined(Result<(), Failure>),
    /// This computer approved another device's request: the code the new device claims
    /// with (identity spec 4.2 step 6).
    Approved(Result<String, Failure>),
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
    /// Setting up came to `setup`; `phrase_kept` is whether the core keeps a phrase that
    /// waits to be confirmed.
    SetUp { setup: Setup, phrase_kept: bool },
    /// The password given to finish setting up was refused, so setting up was not tried.
    SetupRefused(Failure),
    /// The phrase this computer keeps, to show; `None` once it was confirmed.
    Phrase(Result<Option<String>, Failure>),
    /// The three positions of the phrase to ask for.
    Quiz(Result<Vec<u32>, Failure>),
    /// The words given for the quiz were right (`true`: the core forgot the phrase and the
    /// reminders end) or not (`false`: the phrase is shown again).
    Confirmed(Result<bool, Failure>),
    /// The identity was restored from its phrase: this computer is its only device now, and
    /// signed in by it.
    Restored(Result<RosterSummary, Failure>),
    /// The account was moved to a new identity made here (identity spec 4.7), whose phrase
    /// waits to be saved.
    StartedOver(Result<(), Failure>),
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
    /// Where setting up private messages here stands.
    pub setup: Setup,
    /// The core keeps a phrase that waits to be confirmed: the reminders on Home and
    /// Devices show while it does.
    pub phrase_kept: bool,
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

    /// Whether personal conversations can be read and sent from this computer: it holds an
    /// identity, and the account is not known to be bound to another one or to be waiting
    /// for binding. While the check runs, or when it could not be made, an identity held is
    /// taken at its word.
    pub fn private_ready(&self) -> bool {
        self.holds_identity && !self.setup_needed()
    }

    /// Whether the account is known to need something of the person before this computer
    /// has its private messages: linking, restoring or starting over, or the password.
    pub fn setup_needed(&self) -> bool {
        matches!(self.identity.setup, Setup::Elsewhere | Setup::NeedsReauth)
    }

    // ---- Setting up (architecture 5.4) --------------------------------------------------

    /// Sets up private messages on this computer for the signed-in account, silently: the
    /// core makes an identity unless one is held and binds the account under the reauth
    /// the sign-in carried. The answer is [`IdentityUpdate::SetUp`].
    pub fn check_setup(&mut self) {
        let Some(app) = self.client() else { return };
        self.identity.setup = Setup::Checking;
        self.task_local(move || async move {
            let setup = Setup::of(app.identity_setup(DEVICE_LABEL).await);
            let phrase_kept = app.identity_phrase_unconfirmed().await.unwrap_or(false);
            Update::Identity(IdentityUpdate::SetUp { setup, phrase_kept })
        });
    }

    /// Confirms it is this person with the account's password, then sets up again; the
    /// answer is [`IdentityUpdate::SetUp`], or [`IdentityUpdate::SetupRefused`] for a
    /// password the instance refused.
    pub fn set_up_with_password(&mut self, password: &str) {
        let Some(app) = self.client() else { return };
        let password = password.to_string();
        self.identity.setup = Setup::Checking;
        self.task_local(move || async move {
            if let Err(error) = app.reauth_password(&password).await {
                return Update::Identity(IdentityUpdate::SetupRefused(Failure::from(error)));
            }
            let setup = Setup::of(app.identity_setup(DEVICE_LABEL).await);
            let phrase_kept = app.identity_phrase_unconfirmed().await.unwrap_or(false);
            Update::Identity(IdentityUpdate::SetUp { setup, phrase_kept })
        });
    }

    /// Asks for the phrase the core keeps until it is confirmed
    /// ([`IdentityUpdate::Phrase`]).
    pub fn load_phrase(&self) {
        let Some(app) = self.client() else { return };
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Phrase(
                app.identity_phrase().await.map_err(Failure::from),
            ))
        });
    }

    /// Asks for the three positions to check ([`IdentityUpdate::Quiz`]).
    pub fn load_quiz(&self) {
        let Some(app) = self.client() else { return };
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Quiz(
                app.identity_phrase_quiz().await.map_err(Failure::from),
            ))
        });
    }

    /// Checks the words given for the quiz ([`IdentityUpdate::Confirmed`]).
    pub fn confirm_phrase(&self, words: Vec<String>) {
        let Some(app) = self.client() else { return };
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Confirmed(
                app.identity_phrase_confirm(words)
                    .await
                    .map_err(Failure::from),
            ))
        });
    }

    /// Restores the identity from its phrase on this computer, every other device of it
    /// revoked, and signs in by it (identity spec 4.3); the answer is
    /// [`IdentityUpdate::Restored`].
    pub fn restore_identity(&self, phrase: &str) {
        let Some(app) = self.client() else { return };
        let phrase = phrase.split_whitespace().collect::<Vec<_>>().join(" ");
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Restored(
                app.identity_recover(&phrase, DEVICE_LABEL)
                    .await
                    .map_err(Failure::from),
            ))
        });
    }

    /// Starting over (identity spec 4.7): confirms it is this person with the password,
    /// then binds the account to a new identity made here. The answer is
    /// [`IdentityUpdate::StartedOver`].
    pub fn start_over(&self, password: &str) {
        let Some(app) = self.client() else { return };
        let password = password.to_string();
        self.task_local(move || async move {
            let moved = async {
                app.reauth_password(&password).await?;
                app.identity_start_over(DEVICE_LABEL).await?;
                Ok::<_, ClientError>(())
            };
            Update::Identity(IdentityUpdate::StartedOver(
                moved.await.map_err(Failure::from),
            ))
        });
    }

    /// The people this account talks to privately, each once: who would see its key change
    /// if it started over.
    pub fn private_contacts(&self) -> usize {
        let me = self.me();
        let mut others: Vec<&str> = self
            .personal_channels()
            .flat_map(|c| c.participant_account_ids.iter().map(String::as_str))
            .filter(|id| *id != me)
            .collect();
        others.sort_unstable();
        others.dedup();
        others.len()
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

    /// On this computer, which holds the identity: approves another device's link request
    /// (identity spec 4.2), adding it to the roster; the code it claims with arrives as
    /// [`IdentityUpdate::Approved`].
    pub fn approve_link(&self, request: &str) {
        let Some(app) = self.client() else { return };
        let request = request.trim().to_string();
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Approved(
                app.identity_approve(&request).await.map_err(Failure::from),
            ))
        });
    }

    // ---- Linking by a prompt (identity spec 4.2.1) ---------------------------------------

    /// As the new device: opens a link session, so the account's devices that hold the
    /// identity prompt; the answer is [`IdentityUpdate::LinkStarted`].
    pub fn link_start(&self, label: &str) {
        let Some(app) = self.client() else { return };
        let label = label.to_string();
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::LinkStarted(
                app.link_start(&label).await.map_err(Failure::from),
            ))
        });
    }

    /// As the new device: where the session stands, the core taking the next step when it
    /// is this computer's (revealing its secret, claiming the code and signing in by it).
    /// The answer is [`IdentityUpdate::LinkProgressed`], or [`IdentityUpdate::Linked`] with
    /// the roster once it is linked.
    pub fn link_progress(&self) {
        let Some(app) = self.client() else { return };
        self.task_local(move || async move {
            match app.link_progress().await {
                Ok(progress) if progress.stage == "linked" => Update::Identity(
                    IdentityUpdate::Linked(app.identity_show().await.map_err(Failure::from)),
                ),
                answer => Update::Identity(IdentityUpdate::LinkProgressed(
                    answer.map_err(Failure::from),
                )),
            }
        });
    }

    /// As the new device: gives the session up, which declines it.
    pub fn link_cancel(&self) {
        let Some(app) = self.client() else { return };
        self.task_local(move || async move {
            app.link_cancel().await;
            Update::Identity(IdentityUpdate::LinkCancelled)
        });
    }

    /// On this computer, which holds the identity: the link sessions that prompt here
    /// ([`IdentityUpdate::Prompts`]).
    pub fn link_prompts(&self) {
        let Some(app) = self.client() else { return };
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Prompts(
                app.link_prompts().await.map_err(Failure::from),
            ))
        });
    }

    /// Answers a prompt, so the new device reveals and both show their digits
    /// ([`IdentityUpdate::PromptAnswered`]).
    pub fn link_prompt_answer(&self, session_id: &str) {
        let Some(app) = self.client() else { return };
        let session_id = session_id.to_string();
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::PromptAnswered(
                app.link_prompt_answer(&session_id)
                    .await
                    .map_err(Failure::from),
            ))
        });
    }

    /// The digits matched: approves the new device and hands it its code
    /// ([`IdentityUpdate::PromptConfirmed`]).
    pub fn link_prompt_confirm(&self, session_id: &str) {
        let Some(app) = self.client() else { return };
        let session_id = session_id.to_string();
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::PromptConfirmed(
                app.link_prompt_confirm(&session_id)
                    .await
                    .map_err(Failure::from),
            ))
        });
    }

    /// "Not me", or the digits did not match: the session takes nothing more
    /// ([`IdentityUpdate::PromptDeclined`]).
    pub fn link_prompt_decline(&self, session_id: &str) {
        let Some(app) = self.client() else { return };
        let session_id = session_id.to_string();
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::PromptDeclined(
                app.link_prompt_decline(&session_id)
                    .await
                    .map_err(Failure::from),
            ))
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
            IdentityUpdate::Linked(Ok(roster)) | IdentityUpdate::Restored(Ok(roster)) => {
                self.identity.roster = Some(roster.clone());
                self.check_identity();
                self.signed_in = true;
                self.load();
                self.check_setup();
            }
            IdentityUpdate::SetUp { setup, phrase_kept } => {
                self.identity.setup = setup.clone();
                self.identity.phrase_kept = *phrase_kept;
                // Made here just now, or bound: what the stores hold may have changed.
                self.check_identity();
                if *setup == Setup::Ready {
                    self.load_roster();
                    self.refresh_dm_channels();
                }
            }
            IdentityUpdate::SetupRefused(_) => self.identity.setup = Setup::NeedsReauth,
            IdentityUpdate::Phrase(Ok(phrase)) => self.identity.phrase_kept = phrase.is_some(),
            IdentityUpdate::Confirmed(Ok(true)) => self.identity.phrase_kept = false,
            IdentityUpdate::StartedOver(Ok(())) => {
                self.identity.setup = Setup::Ready;
                self.identity.phrase_kept = true;
                self.check_identity();
                self.load_roster();
                self.load();
            }
            IdentityUpdate::Roster(Ok(roster)) | IdentityUpdate::Revoked(Ok(roster)) => {
                self.identity.roster = Some(roster.clone());
            }
            // The new device is in the roster this computer published.
            IdentityUpdate::Approved(Ok(_)) | IdentityUpdate::PromptConfirmed(Ok(())) => {
                self.load_roster();
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
                self.check_setup();
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
                self.hold_snapshot(snapshot.clone());
            }
            IdentityUpdate::LinkRequested(_)
            | IdentityUpdate::LinkStarted(_)
            | IdentityUpdate::LinkProgressed(_)
            | IdentityUpdate::LinkCancelled
            | IdentityUpdate::Prompts(_)
            | IdentityUpdate::PromptAnswered(_)
            | IdentityUpdate::PromptConfirmed(Err(_))
            | IdentityUpdate::PromptDeclined(_)
            | IdentityUpdate::MagicLinkSent(_)
            | IdentityUpdate::Linked(Err(_))
            | IdentityUpdate::Approved(Err(_))
            | IdentityUpdate::Roster(Err(_))
            | IdentityUpdate::Revoked(Err(_))
            | IdentityUpdate::Passkeys(Err(_))
            | IdentityUpdate::PasskeyRemoved(Err(_))
            | IdentityUpdate::Profile(Err(_))
            | IdentityUpdate::ProfileSaved(Err(_))
            | IdentityUpdate::AccountSaved(Err(_))
            | IdentityUpdate::MagicLinkSignedIn(Err(_))
            | IdentityUpdate::Phrase(Err(_))
            | IdentityUpdate::Quiz(_)
            | IdentityUpdate::Confirmed(_)
            | IdentityUpdate::Restored(Err(_))
            | IdentityUpdate::StartedOver(Err(_)) => {}
        }
    }
}
