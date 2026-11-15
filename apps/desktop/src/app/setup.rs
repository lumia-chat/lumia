//! Setting up private messages on this computer (architecture 5.4; identity spec 4.1, 4.3
//! and 4.7): the recovery phrase and its three-word check, restoring from the phrase,
//! starting over, and finishing a setup that needs the password, each with its reactions.
//! Restoring asks first whether a device with the private messages is still at hand (then
//! linking from it is what the person wants), and after the phrase which of the devices
//! restoring would stop to keep ([`RestoreStep`]): restoring revokes every device it does
//! not keep (identity spec 4.3 step 3).
//! The session sets the account up silently after every sign-in (`session::identity`);
//! these are what follows it. The counterpart of the iOS app's setup screens, with their
//! wording for each refusal.
//!
//! Not built: showing the phrase only after this computer's own unlock, since Rune reaches
//! no system prompt (Touch ID, Windows Hello, polkit) and the shell has no other way to
//! ask; and printing it, since Rune has no print dialog. Saving it as a file, behind its
//! warning, is the way to keep it other than writing it down.

use std::path::Path;

use chrono::{DateTime, Local};
use lumia_client::app::DeviceSummary;
use rune::{FileDialogOptions, TextEdit, Tx};

use super::{HomeView, Place, Route, State};
use crate::session::Failure;
use crate::session::identity::{IdentityUpdate, Setup};

/// How many words a phrase has.
pub const PHRASE_WORDS: usize = 24;

/// How long "In a week" puts Home's reminder away, in seconds.
const SNOOZE: i64 = 7 * 24 * 60 * 60;

/// What the setup screens hold between frames.
#[derive(Debug)]
pub struct SetupForms {
    /// Finishing a setup that needs reauth: the account's password.
    pub password: TextEdit,
    /// Waiting for the instance's answer to it.
    pub busy: bool,
    pub error: String,
    pub phrase: PhraseForm,
    pub restore: RestoreForm,
    pub start_over: StartOverForm,
}

impl Default for SetupForms {
    fn default() -> Self {
        Self {
            password: TextEdit::new("").secure(),
            busy: false,
            error: String::new(),
            phrase: PhraseForm::default(),
            restore: RestoreForm::default(),
            start_over: StartOverForm::default(),
        }
    }
}

/// The phrase screen: step 1 shows the words, step 2 asks for three of them.
#[derive(Debug, Default)]
pub struct PhraseForm {
    /// The phrase's words, once the core has given them.
    pub words: Vec<String>,
    /// Waiting for the phrase or the quiz.
    pub loading: bool,
    /// Step 2: the positions asked for (1 to 24), a field each in `answers`.
    pub checking: bool,
    pub quiz: Vec<u32>,
    pub answers: Vec<TextEdit>,
    /// Waiting for the core's answer to the check.
    pub busy: bool,
    /// The last check was wrong, and the phrase is on screen again.
    pub again: bool,
    /// The warning before saving a file is up.
    pub confirming_save: bool,
    /// Where the phrase was saved, once it was.
    pub saved_to: Option<String>,
    pub error: String,
    /// What an edit lands in when its field has gone (the check put away under it).
    spare: TextEdit,
}

impl PhraseForm {
    /// The check's field `n`.
    pub fn answer_mut(&mut self, n: usize) -> &mut TextEdit {
        if n < self.answers.len() {
            &mut self.answers[n]
        } else {
            &mut self.spare
        }
    }
}

/// Which step of restoring is on screen.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RestoreStep {
    /// "Do you still have a device with your private messages?"
    #[default]
    Ask,
    /// The 24 words.
    Phrase,
    /// "Keep any devices?": the devices restoring would stop, to tick those still at hand.
    Keep,
}

/// Restoring from the phrase: the question first, then the 24 words in one field, then
/// the devices to keep. The words are held here, in memory, from the phrase to the keep
/// step, and go with the form when restoring ends or is left.
#[derive(Debug, Default)]
pub struct RestoreForm {
    pub step: RestoreStep,
    pub words: TextEdit,
    /// The devices restoring would stop, as the instance listed them once the phrase was
    /// given.
    pub devices: Vec<DeviceSummary>,
    /// The ones ticked to keep, by id.
    pub kept: Vec<String>,
    /// Waiting for the instances' answers.
    pub busy: bool,
    pub error: String,
}

impl RestoreForm {
    /// How many words the field holds.
    pub fn count(&self) -> usize {
        self.words.text.split_whitespace().count()
    }
}

/// Starting over: the password as reauth.
#[derive(Debug)]
pub struct StartOverForm {
    pub password: TextEdit,
    pub busy: bool,
    pub error: String,
}

impl Default for StartOverForm {
    fn default() -> Self {
        Self {
            password: TextEdit::new("").secure(),
            busy: false,
            error: String::new(),
        }
    }
}

impl State {
    // ---- Finishing a setup ------------------------------------------------------------

    /// Confirms it is this person with the password, and sets up private messages here.
    pub fn finish_setup(&mut self) {
        let form = &mut self.setup;
        if form.busy || form.password.text.is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        self.session.set_up_with_password(&form.password.text);
    }

    /// Asks again after a setup the instance could not be asked for.
    pub fn retry_setup(&mut self) {
        self.setup.error.clear();
        self.session.check_setup();
    }

    // ---- Home's reminder --------------------------------------------------------------

    /// Whether Home shows the reminder to save the phrase at `now` (seconds since 1970):
    /// while the core keeps one, unless "In a week" put it away until later.
    pub fn phrase_reminder(&self, now: i64) -> bool {
        self.session.identity.phrase_kept
            && self
                .settings
                .phrase_reminder_until
                .is_none_or(|until| now >= until)
    }

    /// "In a week": puts Home's reminder away for seven days, on this computer.
    pub fn snooze_phrase_reminder(&mut self) {
        let now = chrono::Utc::now().timestamp();
        self.settings.phrase_reminder_until = Some(now + SNOOZE);
        self.settings.save(&self.data_dir);
    }

    // ---- The phrase -------------------------------------------------------------------

    /// Shows the phrase this computer keeps, from the reminder or from Devices.
    pub fn open_phrase(&mut self) {
        self.setup.phrase = PhraseForm {
            loading: true,
            ..PhraseForm::default()
        };
        self.popover = None;
        self.route = Route::Phrase;
        self.session.load_phrase();
    }

    /// Leaves the phrase screen for the shell, forgetting the words it held.
    pub fn close_phrase(&mut self) {
        self.setup.phrase = PhraseForm::default();
        self.route = Route::Shell;
    }

    /// "I've written them down": step 2, the three words to type.
    pub fn phrase_written(&mut self) {
        let form = &mut self.setup.phrase;
        if form.loading || form.words.is_empty() {
            return;
        }
        form.checking = true;
        form.loading = true;
        form.confirming_save = false;
        form.error.clear();
        self.session.load_quiz();
    }

    /// "Show the phrase again", from step 2.
    pub fn show_phrase_again(&mut self) {
        let form = &mut self.setup.phrase;
        form.checking = false;
        form.error.clear();
    }

    /// Fills a field of the check with a word the wordlist completed.
    pub fn complete_word(&mut self, field: usize, word: &str) {
        if let Some(answer) = self.setup.phrase.answers.get_mut(field) {
            answer.replace_text(word);
        }
    }

    /// Checks the three words typed.
    pub fn confirm_phrase(&mut self) {
        let form = &mut self.setup.phrase;
        if form.busy || form.answers.iter().any(|a| a.text.trim().is_empty()) {
            return;
        }
        form.busy = true;
        form.error.clear();
        let words = form
            .answers
            .iter()
            .map(|a| a.text.trim().to_lowercase())
            .collect();
        self.session.confirm_phrase(words);
    }

    /// "Save as a file": the warning comes first.
    pub fn ask_save_phrase(&mut self) {
        let form = &mut self.setup.phrase;
        form.confirming_save = true;
        form.error.clear();
    }

    /// "Write it down instead": puts the warning away.
    pub fn keep_writing(&mut self) {
        self.setup.phrase.confirming_save = false;
    }

    /// "Save anyway": asks where, and writes the phrase there.
    pub fn save_phrase(&mut self, tx: &mut Tx) {
        let form = &mut self.setup.phrase;
        form.confirming_save = false;
        if form.words.is_empty() {
            return;
        }
        let options = FileDialogOptions {
            title: "Save your recovery phrase".into(),
            file_name: Some("Lumia recovery phrase.txt".into()),
            ..FileDialogOptions::default()
        }
        .filter("Text", &["txt"]);
        tx.save_file(options, |s: &mut State, answer, _| match answer {
            Ok(Some(path)) => s.write_phrase(&path),
            Ok(None) => {}
            Err(error) => {
                s.setup.phrase.error = format!("The file dialog could not open: {error}");
            }
        });
    }

    /// Writes the phrase to `path`, readable by this person's account alone where the
    /// system has such a thing.
    pub fn write_phrase(&mut self, path: &Path) {
        let form = &mut self.setup.phrase;
        match write_private(path, &phrase_file(&form.words)) {
            Ok(()) => {
                form.saved_to = Some(path.display().to_string());
                form.error.clear();
            }
            Err(error) => form.error = format!("The phrase could not be saved: {error}"),
        }
    }

    // ---- Restoring --------------------------------------------------------------------

    /// Shows the restore screen at its question: from sign-in or sign-up, or from a
    /// signed-in computer that needs the identity.
    pub fn start_restore(&mut self) {
        self.setup.restore = RestoreForm::default();
        self.popover = None;
        self.route = Route::Restore;
    }

    /// Leaves the restore screen for where it was opened from.
    pub fn close_restore(&mut self) {
        self.setup.restore = RestoreForm::default();
        self.route = if self.session.signed_in {
            Route::Shell
        } else {
            Route::SignIn
        };
    }

    /// "Yes, link from it": linking, as "Link from a device you use" does from where
    /// restoring was opened.
    pub fn restore_by_linking(&mut self) {
        self.setup.restore = RestoreForm::default();
        self.start_linking();
    }

    /// "No, restore with my phrase": the 24 words.
    pub fn restore_with_phrase(&mut self) {
        let form = &mut self.setup.restore;
        form.step = RestoreStep::Phrase;
        form.error.clear();
    }

    /// Restores with the 24 words in the field: first the devices it would stop are read,
    /// and with none, it restores at once ([`State::react_setup`]).
    pub fn restore_identity(&mut self) {
        let form = &mut self.setup.restore;
        if form.busy || form.step != RestoreStep::Phrase || form.count() != PHRASE_WORDS {
            return;
        }
        form.busy = true;
        form.error.clear();
        self.session.restore_devices(&form.words.text);
    }

    /// Ticks a device to keep, or unticks it.
    pub fn toggle_kept(&mut self, device_id: &str) {
        let form = &mut self.setup.restore;
        if form.busy {
            return;
        }
        if let Some(at) = form.kept.iter().position(|id| id == device_id) {
            form.kept.remove(at);
        } else if form.devices.iter().any(|d| d.device_id == device_id) {
            form.kept.push(device_id.to_string());
        }
    }

    /// Back from the devices to the phrase, its words still in the field.
    pub fn restore_keep_back(&mut self) {
        let form = &mut self.setup.restore;
        if form.busy {
            return;
        }
        form.step = RestoreStep::Phrase;
        form.devices.clear();
        form.kept.clear();
        form.error.clear();
    }

    /// Restores, keeping the devices ticked and stopping the rest.
    pub fn restore_keeping(&mut self) {
        let form = &mut self.setup.restore;
        if form.busy || form.step != RestoreStep::Keep {
            return;
        }
        form.busy = true;
        form.error.clear();
        self.session
            .restore_identity(&form.words.text, form.kept.clone());
    }

    // ---- Starting over ----------------------------------------------------------------

    /// Shows what starting over means, and its confirmation.
    pub fn open_start_over(&mut self) {
        self.setup.start_over = StartOverForm::default();
        self.popover = None;
        self.route = Route::StartOver;
    }

    /// "Keep looking for the phrase": back to the shell.
    pub fn close_start_over(&mut self) {
        self.setup.start_over = StartOverForm::default();
        self.route = Route::Shell;
    }

    /// Starts over with a new identity, the password as reauth.
    pub fn start_over(&mut self) {
        let form = &mut self.setup.start_over;
        if form.busy || form.password.text.is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        self.session.start_over(&form.password.text);
    }

    // ---- What the session reports -----------------------------------------------------

    /// The setup screens' part of an identity update; true when it was theirs.
    pub(super) fn react_setup(&mut self, update: &IdentityUpdate) -> bool {
        match update {
            IdentityUpdate::SetUp { setup, .. } => {
                let form = &mut self.setup;
                form.busy = false;
                if *setup == Setup::Ready {
                    form.password.clear();
                    form.error.clear();
                }
            }
            IdentityUpdate::SetupRefused(failure) => {
                let form = &mut self.setup;
                form.busy = false;
                form.error = password_refusal(failure);
            }
            IdentityUpdate::Phrase(answer) => {
                if self.route != Route::Phrase {
                    return true;
                }
                let form = &mut self.setup.phrase;
                form.loading = false;
                match answer {
                    Ok(Some(phrase)) => {
                        form.words = phrase.split_whitespace().map(str::to_string).collect();
                    }
                    // Confirmed already, on another screen or before.
                    Ok(None) => self.close_phrase(),
                    Err(failure) => form.error.clone_from(&failure.message),
                }
            }
            IdentityUpdate::Quiz(answer) => {
                let form = &mut self.setup.phrase;
                form.loading = false;
                match answer {
                    Ok(positions) => {
                        form.quiz.clone_from(positions);
                        form.answers = positions.iter().map(|_| TextEdit::new("")).collect();
                    }
                    Err(failure) => {
                        form.checking = false;
                        form.error.clone_from(&failure.message);
                    }
                }
            }
            IdentityUpdate::Confirmed(answer) => {
                let form = &mut self.setup.phrase;
                form.busy = false;
                match answer {
                    Ok(true) => {
                        self.close_phrase();
                        if self.place == Place::Home {
                            self.account_opened(self.home_view);
                        }
                    }
                    // A wrong word shows the phrase again; the next check asks for others.
                    Ok(false) => {
                        form.checking = false;
                        form.again = true;
                        form.quiz.clear();
                        form.answers.clear();
                    }
                    Err(failure) => form.error.clone_from(&failure.message),
                }
            }
            IdentityUpdate::RestoreDevices(answer) => {
                let form = &mut self.setup.restore;
                // Left, or gone back, while the instance was asked: nothing waits for it.
                if self.route != Route::Restore || form.step != RestoreStep::Phrase || !form.busy {
                    return true;
                }
                match answer {
                    // Nothing to keep: restored at once, as before there was a choice.
                    Ok(devices) if devices.is_empty() => {
                        self.session.restore_identity(&form.words.text, Vec::new());
                    }
                    Ok(devices) => {
                        form.busy = false;
                        form.step = RestoreStep::Keep;
                        form.devices.clone_from(devices);
                        form.kept.clear();
                    }
                    Err(failure) => {
                        form.busy = false;
                        form.error = restore_refusal(failure);
                    }
                }
            }
            IdentityUpdate::Restored(answer) => match answer {
                Ok(_) => {
                    self.setup.restore = RestoreForm::default();
                    self.signed_in(&Ok(()));
                    if self.place == Place::Home {
                        self.account_opened(self.home_view);
                    }
                }
                Err(failure) => {
                    let form = &mut self.setup.restore;
                    form.busy = false;
                    form.error = restore_refusal(failure);
                }
            },
            IdentityUpdate::StartedOver(answer) => match answer {
                Ok(()) => {
                    self.setup.start_over = StartOverForm::default();
                    self.route = Route::Shell;
                    // The new identity's one device, and the new phrase to save.
                    self.go_home(HomeView::Devices);
                }
                Err(failure) => {
                    let form = &mut self.setup.start_over;
                    form.busy = false;
                    form.error = match failure.code.as_str() {
                        "ALREADY_EXISTS" => {
                            "The identity on this computer belongs to another account on this instance.".into()
                        }
                        _ => password_refusal(failure),
                    };
                }
            },
            _ => return false,
        }
        true
    }
}

/// The keep step's button, by how many devices are ticked.
pub fn keep_button(kept: usize) -> String {
    match kept {
        0 => "Restore and stop them all".into(),
        1 => "Restore and keep 1 device".into(),
        n => format!("Restore and keep {n} devices"),
    }
}

/// The line under a device on the keep step: when it was added, and where a browser's key
/// is scoped. `Added Nov 12`, `On chat.acme.dev · added Nov 1`.
pub fn added_line(device: &DeviceSummary) -> String {
    let day = DateTime::from_timestamp_micros(device.added_at_micros)
        .filter(|_| device.added_at_micros > 0)
        .map(|at| at.with_timezone(&Local).format("%b %-d").to_string());
    let host = device
        .web_origin
        .split_once("://")
        .map_or(device.web_origin.as_str(), |(_, host)| host)
        .trim_end_matches('/');
    match (host.is_empty(), day) {
        (true, Some(day)) => format!("Added {day}"),
        (true, None) => String::new(),
        (false, Some(day)) => format!("On {host} · added {day}"),
        (false, None) => format!("On {host}"),
    }
}

/// A refused reauth, as the other shells word it.
fn password_refusal(failure: &Failure) -> String {
    match failure.code.as_str() {
        "UNAUTHENTICATED" => "That password was not accepted.".into(),
        "RATE_LIMITED" => "Too many tries. Wait a moment and try again.".into(),
        _ => failure.message.clone(),
    }
}

/// A restore that did not happen. The core says which of its checks refused in its own
/// words: a word not in the list, a checksum that does not hold, a phrase rotated away.
fn restore_refusal(failure: &Failure) -> String {
    if failure.code.is_empty() {
        format!(
            "Those words didn't restore an identity: {}. Check each word and their order.",
            failure.message
        )
    } else {
        failure.message.clone()
    }
}

/// What a saved phrase file says: what it is, and the words numbered in order.
pub fn phrase_file(words: &[String]) -> String {
    use std::fmt::Write;
    let mut file = String::from(
        "Lumia recovery phrase\n\n\
         Whoever has these 24 words has your identity and your private messages. \
         Lumia can't recover them for you.\n\n",
    );
    for (n, word) in words.iter().enumerate() {
        let _ = writeln!(file, "{:>2}. {word}", n + 1);
    }
    file
}

/// Writes `contents` to `path`, on Unix readable and writable by its owner alone.
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(contents.as_bytes())
}
