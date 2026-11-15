//! The account screens' state and reactions: linking this computer to an identity, the
//! devices with the revoke confirmation and the passkeys, and the profile with the status.
//! The counterpart of the `@State` in the iOS app's `LinkView.swift` and `Screens.swift`,
//! with their wording for each refusal.
//!
//! Linking this computer, signed in to the account, is by a prompt on the account's devices
//! that hold the identity (identity spec 4.2.1): it opens a session at once, and follows it
//! ([`Prompted`]) on every account event and every two seconds ([`super::prompts::due`])
//! until it ends. From sign-in, with no account signed in to hold a session, it is the
//! copied request and the typed code (4.2), which "Link another way" also offers under the
//! waiting screen.

use std::path::Path;

use lumia_client::app::LinkProgress;
use rune::{FileDialogOptions, TextEdit, Tx};

use super::{HomeView, Place, Popover, Route, State};
use crate::session::Failure;
use crate::session::avatars::AvatarUpdate;
use crate::session::identity::{DEVICE_LABEL, IdentityUpdate};

/// What the account screens hold between frames.
#[derive(Debug, Default)]
pub struct Account {
    pub link: Link,
    pub devices: Devices,
    pub profile: Profile,
    /// The status and custom text the account menu opened with, when the store had one.
    pub menu_status: Option<(String, String)>,
}

/// The link screen: linking by a prompt (identity spec 4.2.1), or the request to show and
/// then the code to claim (4.2).
#[derive(Debug, Default)]
pub struct Link {
    /// Where linking by a prompt stands.
    pub prompted: Prompted,
    /// The copied request and the typed code, chosen under the waiting screen.
    pub another_way: bool,
    /// A question to the instance about the session is on its way; `again` asks once more
    /// when it lands, for an account event that came meanwhile.
    pub asking: bool,
    pub again: bool,
    /// When the session was last asked about, in microseconds.
    pub asked_at: i64,
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

/// Where linking this computer by a prompt stands (identity spec 4.2.1): the core's
/// `LinkProgress` short of `linked`, which goes on as a claimed code does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Prompted {
    /// No session: not started (the other way, or sign-in's), or it could not be opened.
    #[default]
    Idle,
    /// The instance was asked to open one.
    Starting,
    /// Open, and no other device has answered.
    Waiting,
    /// Another device answered: both show these digits (`482 913`).
    Compare(String),
    /// "Not me", or "They don’t match", on the other device.
    Declined,
    /// Ten minutes passed, or the instance no longer knows the session.
    Expired,
}

impl Prompted {
    /// The core's stage, as this screen shows it.
    pub fn of(progress: &LinkProgress) -> Self {
        match progress.stage.as_str() {
            "compare" => Self::Compare(progress.digits.clone()),
            "declined" => Self::Declined,
            "expired" => Self::Expired,
            _ => Self::Waiting,
        }
    }

    /// Whether a session is open and followed: asked about on every account event and
    /// every two seconds.
    pub fn followed(&self) -> bool {
        matches!(self, Self::Waiting | Self::Compare(_))
    }

    /// Whether a session is open or opening, which leaving the screen gives up.
    pub fn open(&self) -> bool {
        matches!(self, Self::Starting | Self::Waiting | Self::Compare(_))
    }

    /// What the card says at this stage.
    pub fn words(&self) -> &'static str {
        match self {
            Self::Idle => "Your other devices couldn’t be asked.",
            Self::Starting | Self::Waiting => {
                "Open Lumia on a device that has your private messages. It will ask about this computer."
            }
            Self::Compare(_) => {
                "Check that your other device shows the same number, then approve it there."
            }
            Self::Declined => "It wasn’t approved on your other device.",
            Self::Expired => "That took too long.",
        }
    }
}

/// The devices screen: the device being revoked with its confirmation, the passkeys, and
/// approving another device's link request.
#[derive(Debug)]
pub struct Devices {
    /// The device whose revocation is being confirmed, by id.
    pub revoking: Option<String>,
    pub password: TextEdit,
    /// Waiting for the instance's answer to a revocation.
    pub busy: bool,
    pub error: String,
    pub passkey_error: String,
    pub approve: Approve,
}

impl Default for Devices {
    fn default() -> Self {
        Self {
            revoking: None,
            password: TextEdit::new("").secure(),
            busy: false,
            error: String::new(),
            passkey_error: String::new(),
            approve: Approve::default(),
        }
    }
}

/// Approving another device (identity spec 4.2, this computer as the device that holds the
/// identity): the request pasted from it, then the code to type there.
#[derive(Debug, Default)]
pub struct Approve {
    /// The card is open.
    pub open: bool,
    pub request: TextEdit,
    /// Waiting for the instance's answer.
    pub busy: bool,
    /// The code the new device claims with, once approved.
    pub code: String,
    pub error: String,
}

/// The profile screen's account card: the handle and, on a computer that does not hold
/// the identity, the display name (`PATCH /account`).
#[derive(Debug, Default)]
pub struct AccountForm {
    pub handle: TextEdit,
    pub display_name: TextEdit,
    /// Waiting for the instance's answer to a save.
    pub busy: bool,
    pub saved: bool,
    pub error: String,
}

/// The account card's photo: choosing one, or taking it away.
#[derive(Debug, Default)]
pub struct PhotoForm {
    /// Waiting for the instance's answer.
    pub busy: bool,
    pub saved: bool,
    pub error: String,
}

/// The profile screen: the account card, the status card and the signed profile's form.
#[derive(Debug)]
pub struct Profile {
    pub account: AccountForm,
    pub photo: PhotoForm,
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
            account: AccountForm::default(),
            photo: PhotoForm::default(),
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
                if let Some(snapshot) = &self.session.snapshot {
                    form.account.handle.replace_text(&snapshot.handle);
                    form.account
                        .display_name
                        .replace_text(&snapshot.display_name);
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
    /// on this computer. Signed in, the account's other devices are asked at once.
    pub fn start_linking(&mut self) {
        self.account.link = Link::default();
        self.popover = None;
        self.route = Route::Link;
        if self.session.signed_in {
            self.start_prompt();
        }
    }

    /// Opens a link session, so the account's devices that hold the identity prompt; "Try
    /// again" after one ended.
    pub fn start_prompt(&mut self) {
        let form = &mut self.account.link;
        if form.prompted.open() {
            return;
        }
        *form = Link {
            prompted: Prompted::Starting,
            ..Link::default()
        };
        self.session.link_start(DEVICE_LABEL);
    }

    /// "Link another way": the copied request and the typed code, the session given up so
    /// the other devices stop asking.
    pub fn link_another_way(&mut self) {
        if self.account.link.prompted.open() {
            self.session.link_cancel();
        }
        self.account.link = Link {
            another_way: true,
            ..Link::default()
        };
    }

    /// Asks where the session stands, unless a question is on its way already (then once
    /// more when it lands). Only while one is open and followed.
    pub(super) fn ask_link_progress(&mut self, now: i64) {
        let form = &mut self.account.link;
        if self.route != Route::Link || !form.prompted.followed() {
            return;
        }
        if form.asking {
            form.again = true;
            return;
        }
        form.asking = true;
        form.asked_at = now;
        self.session.link_progress();
    }

    /// Leaves the link screen for where it was opened from, giving up a session it opened.
    pub fn close_link(&mut self) {
        if self.account.link.prompted.open() {
            self.session.link_cancel();
        }
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

    /// Opens the card that approves another device, empty.
    pub fn open_approve(&mut self) {
        self.account.devices.approve = Approve {
            open: true,
            ..Approve::default()
        };
        self.account.devices.approve.request.focused = true;
    }

    /// Puts the approve card away, code and all.
    pub fn close_approve(&mut self) {
        self.account.devices.approve = Approve::default();
    }

    /// Approves the pasted request, once it reads as one.
    pub fn approve_device(&mut self) {
        let form = &mut self.account.devices.approve;
        let request = form.request.text.trim().to_string();
        if form.busy || lumia_client::app::App::identity_link_request_summary(&request).is_err() {
            return;
        }
        form.error.clear();
        form.busy = true;
        self.session.approve_link(&request);
    }

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

    /// Opens the account menu with the status this person last chose.
    pub fn open_account_menu(&mut self) {
        let chosen = self.session.status();
        self.account.menu_status = chosen.map(|c| (c.status, c.custom_status));
        self.open_popover(Popover::Account);
    }

    /// The account menu's status rows: keeps `status` with the custom text already chosen,
    /// and closes the menu. A refusal is the profile page's to show; the menu has no room.
    pub fn choose_status(&mut self, status: &str) {
        let custom = self
            .account
            .menu_status
            .take()
            .map(|(_, custom)| custom)
            .unwrap_or_default();
        if self.session.set_status(status, &custom).is_ok() {
            self.account.profile.status = status.to_string();
        }
        self.close_popover();
    }

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

    /// Whether the account card has the display name: on a computer that holds the
    /// identity the name is the signed profile's to carry, in the card under it.
    pub fn names_account(&self) -> bool {
        !(self.account.profile.loaded || self.session.has_identity())
    }

    /// Sends the account card: the handle, and the display name where the card has it.
    pub fn save_account(&mut self) {
        let names = self.names_account();
        let form = &mut self.account.profile.account;
        if form.busy || form.handle.text.trim().is_empty() {
            return;
        }
        form.busy = true;
        form.saved = false;
        form.error.clear();
        self.session.save_account(
            &form.handle.text,
            names.then_some(form.display_name.text.as_str()),
        );
    }

    /// Asks for a picture file and makes it this account's photo.
    pub fn choose_photo(&mut self, tx: &mut Tx) {
        if self.account.profile.photo.busy {
            return;
        }
        let options = FileDialogOptions {
            title: "Choose a photo".into(),
            ..FileDialogOptions::default()
        }
        .filter("Pictures", &["png", "jpg", "jpeg", "webp", "gif"]);
        tx.open_file(options, |s: &mut State, answer, _| match answer {
            Ok(Some(path)) => s.set_photo(&path),
            Ok(None) => {}
            Err(error) => {
                s.account.profile.photo.error = format!("The file dialog could not open: {error}");
            }
        });
    }

    /// Sends the file at `path` to be this account's photo.
    pub fn set_photo(&mut self, path: &Path) {
        let form = &mut self.account.profile.photo;
        if form.busy {
            return;
        }
        *form = PhotoForm {
            busy: true,
            ..PhotoForm::default()
        };
        self.session.set_avatar(path);
    }

    /// Takes this account's photo away.
    pub fn remove_photo(&mut self) {
        let form = &mut self.account.profile.photo;
        if form.busy {
            return;
        }
        *form = PhotoForm {
            busy: true,
            ..PhotoForm::default()
        };
        self.session.remove_avatar();
    }

    pub(super) fn react_avatar(&mut self, update: &AvatarUpdate) {
        let AvatarUpdate::Saved(answer) = update else {
            return;
        };
        let form = &mut self.account.profile.photo;
        *form = PhotoForm::default();
        match answer {
            Ok(_) => form.saved = true,
            Err(failure) => form.error = picture_refusal(failure),
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
        if self.react_prompts(update) || self.react_setup(update) {
            return;
        }
        match update {
            IdentityUpdate::LinkStarted(answer) => {
                let form = &mut self.account.link;
                // Given up meanwhile: the session it opened is declined too.
                if form.prompted != Prompted::Starting {
                    if answer.is_ok() {
                        self.session.link_cancel();
                    }
                    return;
                }
                match answer {
                    Ok(progress) => {
                        form.prompted = Prompted::of(progress);
                        form.asked_at = chrono::Utc::now().timestamp_micros();
                    }
                    Err(failure) => {
                        form.prompted = Prompted::Idle;
                        form.error = link_refusal(failure);
                    }
                }
            }
            IdentityUpdate::LinkProgressed(answer) => {
                let form = &mut self.account.link;
                form.asking = false;
                if !form.prompted.followed() {
                    return;
                }
                match answer {
                    Ok(progress) => {
                        form.prompted = Prompted::of(progress);
                        form.error.clear();
                    }
                    // The core holds no session: it ended, or was given up elsewhere.
                    Err(failure) if failure.message == "no link in progress" => {
                        form.prompted = Prompted::Expired;
                    }
                    // Asked again in two seconds; the words stay until an answer comes.
                    Err(failure) => form.error.clone_from(&failure.message),
                }
                if std::mem::take(&mut form.again) {
                    self.ask_link_progress(chrono::Utc::now().timestamp_micros());
                }
            }
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
                form.asking = false;
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
            IdentityUpdate::Approved(answer) => {
                let form = &mut self.account.devices.approve;
                form.busy = false;
                match answer {
                    Ok(code) => code.clone_into(&mut form.code),
                    Err(failure) => {
                        // A device past the roster's twenty (identity spec 11) is a
                        // roster the instance refuses to parse.
                        let full = failure.details.get("reason").map(String::as_str)
                            == Some("too_many_devices");
                        form.error = if full {
                            "Your identity already has as many devices as it can hold. Revoke one first."
                                .into()
                        } else {
                            failure.message.clone()
                        };
                    }
                }
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
            IdentityUpdate::AccountSaved(answer) => {
                let form = &mut self.account.profile.account;
                form.busy = false;
                match answer {
                    Ok(snapshot) => {
                        form.saved = true;
                        // As the instance keeps them: trimmed.
                        form.handle.replace_text(&snapshot.handle);
                        form.display_name.replace_text(&snapshot.display_name);
                    }
                    Err(failure) => {
                        form.error = match failure.code.as_str() {
                            "ALREADY_EXISTS" => "That handle is taken.".into(),
                            "INVALID_ARGUMENT" => "A handle is 2 to 32 letters, digits, _, . or -, and a display name at most 64 characters.".into(),
                            _ => failure.message.clone(),
                        }
                    }
                }
            }
            IdentityUpdate::MagicLinkSent(answer) => self.magic_link_sent(answer),
            IdentityUpdate::MagicLinkSignedIn(answer) => self.magic_link_signed_in(answer),
            // The session keeps these; the screens read them from it. The setup screens took
            // theirs above.
            IdentityUpdate::Roster(_)
            | IdentityUpdate::Passkeys(_)
            | IdentityUpdate::PasskeyRemoved(Ok(()))
            | IdentityUpdate::SetUp { .. }
            | IdentityUpdate::SetupRefused(_)
            | IdentityUpdate::LinkCancelled
            | IdentityUpdate::Prompts(_)
            | IdentityUpdate::PromptAnswered(_)
            | IdentityUpdate::PromptConfirmed(_)
            | IdentityUpdate::PromptDeclined(_)
            | IdentityUpdate::Phrase(_)
            | IdentityUpdate::Quiz(_)
            | IdentityUpdate::Confirmed(_)
            | IdentityUpdate::Restored(_)
            | IdentityUpdate::StartedOver(_) => {}
        }
    }
}

/// Why a link session could not be opened.
fn link_refusal(failure: &Failure) -> String {
    match failure.code.as_str() {
        // At most five open at once for an account (identity spec 4.2.1).
        "RATE_LIMITED" => {
            "Too many devices are waiting to be linked. Try again in a few minutes.".into()
        }
        _ => failure.message.clone(),
    }
}

/// Why a picture was not taken, an account's or a server's, as the other shells word it.
pub(super) fn picture_refusal(failure: &Failure) -> String {
    let reason = failure.details.get("reason").map(String::as_str);
    match (failure.code.as_str(), reason) {
        ("INVALID_ARGUMENT", Some("too_large")) => {
            "That picture is too large. Choose one under 8 MB.".into()
        }
        ("INVALID_ARGUMENT", Some("unsupported")) => {
            "That file is not a picture Lumia can read. Choose a PNG, JPEG, WebP or GIF.".into()
        }
        ("INVALID_ARGUMENT", Some("unreadable")) => {
            format!("That file could not be read: {}.", failure.message)
        }
        ("INVALID_ARGUMENT", _) => "That picture could not be read.".into(),
        ("FAILED_PRECONDITION", _) => "This instance does not keep pictures.".into(),
        ("RATE_LIMITED", _) => "The instance is busy. Try again in a moment.".into(),
        _ => failure.message.clone(),
    }
}
