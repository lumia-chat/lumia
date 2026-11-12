//! A server's settings (architecture 6.7): its name, its picture and how long it keeps its
//! messages, changed by its owner or a member whose roles grant `MANAGE_SERVER`. The page is
//! offered only to them (`ServerSummary::can_manage`, from the account's own membership);
//! the instance decides all the same. Each card saves on its own and says so, worded as the
//! web's and the phones'.
//!
//! Shortening how long messages are kept deletes, at the instance's next daily run and for
//! everyone, what is older than the new limit; the card says so before it is saved.

use std::path::Path;

use rune::{FileDialogOptions, TextEdit, Tx};

use super::State;
use super::account::picture_refusal;
use crate::session::Failure;
use crate::session::servers::Setting;

/// The retention choices the page offers, in days; 0 keeps messages for ever.
pub const RETENTION_CHOICES: [u32; 4] = [0, 365, 90, 30];

/// What a retention reads as: "Forever", "1 year", "90 days".
pub fn retention_words(days: u32) -> String {
    match days {
        0 => "Forever".into(),
        365 => "1 year".into(),
        1 => "1 day".into(),
        n => format!("{n} days"),
    }
}

/// The choices to draw for a server whose retention is `current`: the usual four, and the
/// current one too when it was set elsewhere to something else.
pub fn retention_choices(current: u32) -> Vec<u32> {
    let mut choices = RETENTION_CHOICES.to_vec();
    if !choices.contains(&current) {
        choices.push(current);
    }
    choices
}

/// Whether keeping messages for `chosen` days instead of `current` deletes some: for ever
/// is the longest.
pub fn shortens(current: u32, chosen: u32) -> bool {
    chosen != 0 && (current == 0 || chosen < current)
}

/// One card's save: waiting, done, or why not.
#[derive(Debug, Default, Clone)]
pub struct Saving {
    pub busy: bool,
    pub saved: bool,
    pub error: String,
}

impl Saving {
    fn started() -> Self {
        Self {
            busy: true,
            ..Self::default()
        }
    }
}

/// The settings page: which server it is open for, and each card's form.
#[derive(Debug, Default)]
pub struct ServerSettings {
    /// The server the page is open for; none when it is not on screen.
    pub server_id: Option<String>,
    pub name: TextEdit,
    pub name_saving: Saving,
    /// The retention chosen on the page, in days; 0 for ever.
    pub retention: u32,
    pub retention_saving: Saving,
    pub picture: Saving,
}

/// Why a setting was not changed.
fn setting_refusal(failure: &Failure) -> String {
    match failure.code.as_str() {
        "PERMISSION_DENIED" => "You cannot change this server's settings.".into(),
        "INVALID_ARGUMENT" if failure.details.get("field").map(String::as_str) == Some("name") => {
            "A server name is 1 to 100 characters.".into()
        }
        _ => failure.message.clone(),
    }
}

impl State {
    /// Whether the server on screen shows its settings instead of a channel.
    pub fn server_settings_open(&self) -> bool {
        self.server_forms.settings.server_id.as_deref() == self.server_id()
            && self.server_id().is_some()
    }

    /// Opens the settings of the server on screen, from what the snapshot says it is.
    pub fn open_server_settings(&mut self) {
        let Some(server) = self
            .server_id()
            .and_then(|id| self.session.servers().iter().find(|s| s.server_id == id))
            .cloned()
        else {
            return;
        };
        self.close_popover();
        self.server_forms.settings = ServerSettings {
            server_id: Some(server.server_id.clone()),
            name: TextEdit::new(&server.name),
            retention: server.retention_days,
            ..ServerSettings::default()
        };
    }

    /// Back to the channel the page was opened over.
    pub fn close_server_settings(&mut self) {
        self.server_forms.settings.server_id = None;
    }

    /// The server the page is open for, as the snapshot has it.
    fn settings_server(&self) -> Option<String> {
        self.server_settings_open()
            .then(|| self.server_forms.settings.server_id.clone())
            .flatten()
    }

    pub fn save_server_name(&mut self) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        let form = &mut self.server_forms.settings;
        let name = form.name.text.trim().to_string();
        if form.name_saving.busy || name.is_empty() {
            return;
        }
        form.name_saving = Saving::started();
        self.session.rename_server(&server_id, &name);
    }

    pub fn choose_retention(&mut self, days: u32) {
        let form = &mut self.server_forms.settings;
        form.retention = days;
        form.retention_saving.saved = false;
    }

    pub fn save_server_retention(&mut self) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        let form = &mut self.server_forms.settings;
        if form.retention_saving.busy {
            return;
        }
        form.retention_saving = Saving::started();
        let days = form.retention;
        self.session.set_server_retention(&server_id, days);
    }

    /// Asks the system for a picture file; the one chosen is sent at once.
    pub fn choose_server_picture(&mut self, tx: &mut Tx) {
        if self.server_forms.settings.picture.busy {
            return;
        }
        let options = FileDialogOptions {
            title: "Choose a picture".into(),
            ..FileDialogOptions::default()
        }
        .filter("Pictures", &["png", "jpg", "jpeg", "webp", "gif"]);
        tx.open_file(options, |s: &mut State, answer, _| match answer {
            Ok(Some(path)) => s.set_server_picture(&path),
            Ok(None) => {}
            Err(error) => {
                s.server_forms.settings.picture.error =
                    format!("The file dialog could not open: {error}");
            }
        });
    }

    /// Sends the file at `path` to be the picture of the server whose settings are open.
    pub fn set_server_picture(&mut self, path: &Path) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        if self.server_forms.settings.picture.busy {
            return;
        }
        self.server_forms.settings.picture = Saving::started();
        self.session.set_server_icon(&server_id, path);
    }

    pub fn remove_server_picture(&mut self) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        if self.server_forms.settings.picture.busy {
            return;
        }
        self.server_forms.settings.picture = Saving::started();
        self.session.remove_server_icon(&server_id);
    }

    /// A setting's answer, for the card that asked, when the page is still open for that
    /// server.
    pub(super) fn react_setting(
        &mut self,
        server_id: &str,
        setting: Setting,
        result: &Result<lumia_client::app::SnapshotSummary, Failure>,
    ) {
        let form = &mut self.server_forms.settings;
        if form.server_id.as_deref() != Some(server_id) {
            return;
        }
        let saving = match setting {
            Setting::Name => &mut form.name_saving,
            Setting::Retention => &mut form.retention_saving,
            Setting::Picture => &mut form.picture,
        };
        *saving = Saving::default();
        match result {
            Ok(snapshot) => {
                saving.saved = true;
                if let Some(server) = snapshot.servers.iter().find(|s| s.server_id == server_id) {
                    match setting {
                        Setting::Name => form.name = TextEdit::new(&server.name),
                        Setting::Retention => form.retention = server.retention_days,
                        Setting::Picture => {}
                    }
                }
            }
            Err(failure) => {
                saving.error = match setting {
                    Setting::Picture if failure.code != "PERMISSION_DENIED" => {
                        picture_refusal(failure)
                    }
                    _ => setting_refusal(failure),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_reads_and_shortens_as_people_say_it() {
        assert_eq!(retention_words(0), "Forever");
        assert_eq!(retention_words(365), "1 year");
        assert_eq!(retention_words(90), "90 days");
        assert_eq!(retention_choices(90), vec![0, 365, 90, 30]);
        // Set elsewhere to something the page does not offer: shown, so it can be kept.
        assert_eq!(retention_choices(14), vec![0, 365, 90, 30, 14]);
        // For ever is the longest; a shorter time deletes, a longer one does not.
        assert!(shortens(0, 30));
        assert!(shortens(90, 30));
        assert!(!shortens(30, 90));
        assert!(!shortens(30, 0));
        assert!(!shortens(30, 30));
    }
}
