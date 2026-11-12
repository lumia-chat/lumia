//! The account screens' state and reactions: linking this computer to an identity, the
//! devices with the revoke confirmation and the passkeys, and the profile with the status.
//! The counterpart of the `@State` in `LinkView.swift` and in `HomeViews.swift`'s
//! `DevicesView` and `ProfileView`, with their wording for each refusal.

use rune::{TextEdit, Tx};

use super::{HomeView, Place, Route, State};
use crate::session::identity::IdentityUpdate;

/// The label this computer's device carries in a link request, as the other desktop
/// shells send it.
const DEVICE_LABEL: &str = "desktop";

/// What the account screens hold between frames.
#[derive(Debug, Default)]
pub struct Account {
    pub link: Link,
    pub devices: Devices,
    pub profile: Profile,
}

/// The link screen (identity spec 4.2): the request to show, then the code to claim.
#[derive(Debug, Default)]
pub struct Link {
    /// The request a device that holds the identity approves; empty until started.
    pub request: String,
    pub code: TextEdit,
    /// Waiting for the request to be made.
    pub starting: bool,
    /// Waiting for the instance's answer to the claim.
    pub busy: bool,
    /// The request went to the clipboard since it was made.
    pub copied: bool,
    pub error: String,
}

/// The devices screen: the device being revoked with its confirmation, and the passkeys.
#[derive(Debug)]
pub struct Devices {
    /// The device whose revocation is being confirmed, by id.
    pub revoking: Option<String>,
    pub password: TextEdit,
    /// Waiting for the instance's answer to a revocation.
    pub busy: bool,
    pub error: String,
    pub passkey_error: String,
}

impl Default for Devices {
    fn default() -> Self {
        Self {
            revoking: None,
            password: TextEdit::new("").secure(),
            busy: false,
            error: String::new(),
            passkey_error: String::new(),
        }
    }
}

/// The profile screen: the status card and the signed profile's form.
#[derive(Debug)]
pub struct Profile {
    /// `ONLINE`, `AWAY` or `DO_NOT_DISTURB` (protocol 6.7).
    pub status: String,
    pub custom_status: TextEdit,
    pub status_saved: bool,
    pub status_error: String,
    /// The profile held has landed. The form waits for it: fields shown before would be
    /// overwritten by it, typing and all.
    pub loaded: bool,
    pub display_name: TextEdit,
    pub pronouns: TextEdit,
    pub bio: TextEdit,
    /// Waiting for the instance's answer to a save.
    pub busy: bool,
    pub saved: bool,
    pub error: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            status: "ONLINE".into(),
            custom_status: TextEdit::default(),
            status_saved: false,
            status_error: String::new(),
            loaded: false,
            display_name: TextEdit::default(),
            pronouns: TextEdit::default(),
            bio: TextEdit::default(),
            busy: false,
            saved: false,
            error: String::new(),
        }
    }
}

impl State {
    /// A Home view came on screen: the account screens ask for what they show, as the
    /// other shells do when theirs appear.
    pub(super) fn account_opened(&mut self, view: HomeView) {
        match view {
            HomeView::Devices => {
                self.account.devices = Devices::default();
                self.session.load_roster();
                self.session.load_passkeys();
            }
            HomeView::Profile => {
                let mut form = Profile::default();
                if let Some(chosen) = self.session.status() {
                    form.status = chosen.status;
                    form.custom_status.replace_text(&chosen.custom_status);
                }
                self.account.profile = form;
                if self.session.has_identity() {
                    self.session.load_profile();
                }
            }
            _ => {}
        }
    }

    // ---- Linking this computer ----------------------------------------------------------

    /// Shows the link screen, from sign-in or from a signed-in account with no identity
    /// on this computer.
    pub fn start_linking(&mut self) {
        self.account.link = Link::default();
        self.popover = None;
        self.route = Route::Link;
    }

    /// Leaves the link screen for where it was opened from.
    pub fn close_link(&mut self) {
        self.account.link = Link::default();
        self.route = if self.session.signed_in {
            Route::Shell
        } else {
            Route::SignIn
        };
    }

    /// Step 1: makes this computer's device key and the request to approve elsewhere.
    pub fn start_link(&mut self) {
        let form = &mut self.account.link;
        if form.starting {
            return;
        }
        form.error.clear();
        form.starting = true;
        self.session.request_link(DEVICE_LABEL);
    }

    /// Puts the request on the clipboard, to carry to the approving device.
    pub fn copy_link_request(&mut self, tx: &mut Tx) {
        let form = &mut self.account.link;
        if form.request.is_empty() {
            return;
        }
        tx.copy(form.request.clone());
        form.copied = true;
    }

    /// Step 7: claims the code the approving device gave, and signs in by identity proof.
    pub fn claim_link(&mut self) {
        let form = &mut self.account.link;
        if form.busy || form.code.text.trim().is_empty() {
            return;
        }
        form.error.clear();
        form.busy = true;
        self.session.claim_link(&form.code.text);
    }

    // ---- Devices ------------------------------------------------------------------------

    /// Asks to confirm revoking a device with the account password.
    pub fn ask_revoke(&mut self, device_id: &str) {
        let form = &mut self.account.devices;
        form.revoking = Some(device_id.to_string());
        form.password.clear();
        form.password.focused = true;
        form.error.clear();
        form.busy = false;
    }

    /// Puts the confirmation away.
    pub fn keep_device(&mut self) {
        let form = &mut self.account.devices;
        form.revoking = None;
        form.password.clear();
        form.error.clear();
        form.busy = false;
    }

    /// Revokes the device being confirmed, with the password as reauth.
    pub fn revoke(&mut self) {
        let form = &mut self.account.devices;
        let Some(device_id) = form.revoking.clone() else {
            return;
        };
        if form.busy || form.password.text.is_empty() {
            return;
        }
        form.error.clear();
        form.busy = true;
        self.session.revoke_device(&device_id, &form.password.text);
    }

    pub fn remove_passkey(&mut self, passkey_id: &str) {
        self.account.devices.passkey_error.clear();
        self.session.remove_passkey(passkey_id);
    }

    // ---- Profile and status -------------------------------------------------------------

    pub fn pick_status(&mut self, status: &str) {
        let form = &mut self.account.profile;
        form.status = status.to_string();
        form.status_saved = false;
    }

    /// Keeps the chosen status and custom text; the core sends it with the servers watched.
    pub fn save_status(&mut self) {
        let form = &mut self.account.profile;
        form.status_saved = false;
        form.status_error.clear();
        match self
            .session
            .set_status(&form.status, &form.custom_status.text)
        {
            Ok(()) => form.status_saved = true,
            Err(error) => form.status_error = error,
        }
    }

    /// Signs the form as a new profile version and sends it.
    pub fn save_profile(&mut self) {
        let form = &mut self.account.profile;
        if form.busy || !form.loaded {
            return;
        }
        form.busy = true;
        form.saved = false;
        form.error.clear();
        self.session
            .save_profile(&form.display_name.text, &form.pronouns.text, &form.bio.text);
    }

    // ---- What the session reports -------------------------------------------------------

    pub(super) fn react_identity(&mut self, update: &IdentityUpdate) {
        match update {
            IdentityUpdate::LinkRequested(answer) => {
                let form = &mut self.account.link;
                form.starting = false;
                match answer {
                    Ok(request) => {
                        form.request = request.clone();
                        form.copied = false;
                    }
                    Err(failure) => form.error = failure.message.clone(),
                }
            }
            IdentityUpdate::Linked(Ok(_)) => {
                self.account.link = Link::default();
                self.signed_in(&Ok(()));
                // The page the link screen was opened from has an identity to show now.
                if self.place == Place::Home {
                    self.account_opened(self.home_view);
                }
            }
            IdentityUpdate::Linked(Err(failure)) => {
                let form = &mut self.account.link;
                form.busy = false;
                // A code is eight characters, good once, for ten minutes, and gone after
                // five wrong tries (identity spec 6); the instance says only "not found".
                form.error = match failure.code.as_str() {
                    "NOT_FOUND" => {
                        "That code is unknown, used or expired. Approve the request again for a new one."
                            .into()
                    }
                    _ => failure.message.clone(),
                };
            }
            IdentityUpdate::Revoked(answer) => {
                let form = &mut self.account.devices;
                form.busy = false;
                match answer {
                    Ok(_) => {
                        form.revoking = None;
                        form.password.clear();
                    }
                    Err(failure) => {
                        form.error = match failure.code.as_str() {
                            "UNAUTHENTICATED" => "That password was not accepted.".into(),
                            _ => failure.message.clone(),
                        }
                    }
                }
            }
            IdentityUpdate::PasskeyRemoved(Err(failure)) => {
                self.account.devices.passkey_error = failure.message.clone();
            }
            IdentityUpdate::Profile(answer) => {
                let form = &mut self.account.profile;
                if form.loaded {
                    // The form is in use: a late answer does not take it from under the
                    // person typing.
                    return;
                }
                form.loaded = true;
                match answer {
                    Ok(Some(profile)) => {
                        form.display_name.replace_text(&profile.display_name);
                        form.pronouns.replace_text(&profile.pronouns);
                        form.bio.replace_text(&profile.bio);
                    }
                    Ok(None) => {}
                    Err(failure) => form.error = failure.message.clone(),
                }
            }
            IdentityUpdate::ProfileSaved(answer) => {
                let form = &mut self.account.profile;
                form.busy = false;
                match answer {
                    Ok(_) => form.saved = true,
                    Err(failure) => {
                        form.error = match failure.code.as_str() {
                            "FAILED_PRECONDITION" => "This account is not linked to your identity, so the instance keeps the profile it has.".into(),
                            "CONFLICT" => "A newer profile is already held. Reload and try again.".into(),
                            _ => failure.message.clone(),
                        }
                    }
                }
            }
            IdentityUpdate::MagicLinkSent(answer) => self.magic_link_sent(answer),
            IdentityUpdate::MagicLinkSignedIn(answer) => self.magic_link_signed_in(answer),
            // The session keeps these; the screens read them from it.
            IdentityUpdate::Roster(_)
            | IdentityUpdate::Passkeys(_)
            | IdentityUpdate::PasskeyRemoved(Ok(())) => {}
        }
    }
}
