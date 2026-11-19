//! A server's settings (architecture 6.7 and 7.5), in sections: Overview (its name, its
//! picture, the channels newcomers join, how long it keeps its messages and files, and what
//! the tag beside a name shows), Roles and Members (`role_settings`). A section is offered to
//! whoever may use it ([`roles::settings_sections`]: Manage server, Manage roles, Manage
//! members), the page to anyone with at least one, and it opens on the first they have; the
//! instance decides all the same. Each of Overview's cards saves on its own and says so,
//! worded as the web's and the phones'; "Tag beside names" is saved as it is chosen.
//!
//! Shortening how long messages are kept deletes, at the instance's next daily run and for
//! everyone, what is older than the new limit; the card says so before it is saved. The
//! Files card does the same for attachments, says what the server's files take, and says
//! what letting the oldest go means before it is chosen (specs/media.md 4.5).

use std::path::Path;

use rune::{FileDialogOptions, TextEdit, Tx};

use super::State;
use super::account::picture_refusal;
use super::roles::{self, Section};
use crate::session::Failure;
use crate::session::media::size_words;
use crate::session::servers::Setting;
use lumia_client::app::StorageSummary;

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

/// What a server's files take, as its settings say it: "1.2 GB of 5 GB used", or "1.2 GB
/// used" where the instance sets no limit.
pub fn usage_words(storage: &StorageSummary) -> String {
    if storage.max_bytes == 0 {
        format!("{} used", size_words(storage.used_bytes))
    } else {
        format!(
            "{} of {} used",
            size_words(storage.used_bytes),
            size_words(storage.max_bytes)
        )
    }
}

/// One card's save: waiting, done, or why not.
#[derive(Debug, Default, Clone)]
pub struct Saving {
    pub busy: bool,
    pub saved: bool,
    pub error: String,
}

impl Saving {
    pub(super) fn started() -> Self {
        Self {
            busy: true,
            ..Self::default()
        }
    }
}

/// The settings page: which server it is open for, and each card's form.
#[derive(Debug)]
pub struct ServerSettings {
    /// The server the page is open for; none when it is not on screen.
    pub server_id: Option<String>,
    /// The section shown.
    pub section: Section,
    /// "Tag beside names": its save, and the choice on its way to the instance.
    pub tag: Saving,
    pub tag_choice: Option<bool>,
    pub name: TextEdit,
    pub name_saving: Saving,
    /// The retention chosen on the page, in days; 0 for ever.
    pub retention: u32,
    pub retention_saving: Saving,
    pub picture: Saving,
    pub show_deleted: Saving,
    /// How long attachments are kept, in days (0 for ever), and whether a full server lets
    /// its oldest go, as chosen on the page; saved together.
    pub attachment_retention: u32,
    pub delete_oldest_when_full: bool,
    pub files_saving: Saving,
    /// What the server's files take, once the instance said; why not, when it could not.
    pub storage: Option<StorageSummary>,
    pub storage_error: String,
}

impl Default for ServerSettings {
    // By hand, the core's sections having no default: a page not open shows Overview.
    fn default() -> Self {
        Self {
            server_id: None,
            section: Section::Overview,
            tag: Saving::default(),
            tag_choice: None,
            name: TextEdit::default(),
            name_saving: Saving::default(),
            retention: 0,
            retention_saving: Saving::default(),
            picture: Saving::default(),
            show_deleted: Saving::default(),
            attachment_retention: 0,
            delete_oldest_when_full: false,
            files_saving: Saving::default(),
            storage: None,
            storage_error: String::new(),
        }
    }
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

    /// Whether the server on screen has settings this account may use: at least one section.
    pub fn settings_offered(&self) -> bool {
        self.server_id()
            .and_then(|id| self.session.server(id))
            .is_some_and(|server| !roles::settings_sections(server).is_empty())
    }

    /// Opens the settings of the server on screen, from what the snapshot says it is, on the
    /// first section this account has there.
    pub fn open_server_settings(&mut self) {
        let Some(server) = self
            .server_id()
            .and_then(|id| self.session.server(id))
            .cloned()
        else {
            return;
        };
        let sections = roles::settings_sections(&server);
        let Some(section) = sections.first().copied() else {
            return;
        };
        self.close_popover();
        self.close_page();
        self.roles = super::role_settings::RoleForms::default();
        self.server_forms.settings = ServerSettings {
            server_id: Some(server.server_id.clone()),
            section,
            name: TextEdit::new(&server.name),
            retention: server.retention_days,
            attachment_retention: server.attachment_retention_days,
            delete_oldest_when_full: server.delete_oldest_when_full,
            ..ServerSettings::default()
        };
        // Whoever joined since the members were last listed is there to manage and to count.
        self.session.load_members_of(&server.server_id);
        // What Overview shows is Manage server's to read.
        if sections.contains(&Section::Overview) {
            self.load_defaults(&server.server_id);
            self.session.load_server_storage(&server.server_id);
        }
    }

    /// Back to the channel the page was opened over.
    pub fn close_server_settings(&mut self) {
        if self.server_forms.settings.server_id.take().is_some() {
            // The card's own form is not the page's: it stays.
            let card = self.roles.card.take();
            self.roles = super::role_settings::RoleForms::default();
            self.roles.card = card;
        }
    }

    /// The server the page is open for.
    pub(super) fn settings_server(&self) -> Option<String> {
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

    pub fn choose_attachment_retention(&mut self, days: u32) {
        let form = &mut self.server_forms.settings;
        form.attachment_retention = days;
        form.files_saving.saved = false;
    }

    pub fn choose_when_full(&mut self, delete_oldest: bool) {
        let form = &mut self.server_forms.settings;
        form.delete_oldest_when_full = delete_oldest;
        form.files_saving.saved = false;
    }

    pub fn save_server_files(&mut self) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        let form = &mut self.server_forms.settings;
        if form.files_saving.busy {
            return;
        }
        form.files_saving = Saving::started();
        let (days, delete_oldest) = (form.attachment_retention, form.delete_oldest_when_full);
        self.session
            .set_server_files(&server_id, days, delete_oldest);
    }

    /// What the server's files take, when the page is still open for it.
    pub(super) fn react_storage(
        &mut self,
        server_id: &str,
        result: &Result<StorageSummary, Failure>,
    ) {
        let form = &mut self.server_forms.settings;
        if form.server_id.as_deref() != Some(server_id) {
            return;
        }
        match result {
            Ok(storage) => form.storage = Some(*storage),
            Err(failure) => form.storage_error = setting_refusal(failure),
        }
    }

    /// Turns "Show deleted messages" over, saved at once (architecture 6.7).
    pub fn toggle_show_deleted(&mut self) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        if self.server_forms.settings.show_deleted.busy {
            return;
        }
        let Some(show) = self
            .session
            .servers()
            .iter()
            .find(|s| s.server_id == server_id)
            .map(|s| !s.show_deleted)
        else {
            return;
        };
        self.server_forms.settings.show_deleted = Saving::started();
        self.session.set_show_deleted(&server_id, show);
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
            Setting::ShowDeleted => &mut form.show_deleted,
            Setting::Files => &mut form.files_saving,
            Setting::Tag => {
                form.tag_choice = None;
                &mut form.tag
            }
        };
        *saving = Saving::default();
        match result {
            Ok(snapshot) => {
                saving.saved = true;
                if let Some(server) = snapshot.servers.iter().find(|s| s.server_id == server_id) {
                    match setting {
                        Setting::Name => form.name = TextEdit::new(&server.name),
                        Setting::Retention => form.retention = server.retention_days,
                        Setting::Files => {
                            form.attachment_retention = server.attachment_retention_days;
                            form.delete_oldest_when_full = server.delete_oldest_when_full;
                        }
                        Setting::Picture | Setting::ShowDeleted | Setting::Tag => {}
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
    }

    #[test]
    fn usage_names_the_limit_only_where_there_is_one() {
        let of = |used_bytes, max_bytes| StorageSummary {
            used_bytes,
            max_bytes,
        };
        assert_eq!(usage_words(&of(1_200_000_000, 0)), "1.2 GB used");
        assert_eq!(
            usage_words(&of(1_200_000_000, 5_000_000_000)),
            "1.2 GB of 5 GB used"
        );
        assert_eq!(usage_words(&of(0, 0)), "0 bytes used");
        // For ever is the longest; a shorter time deletes, a longer one does not.
        assert!(shortens(0, 30));
        assert!(shortens(90, 30));
        assert!(!shortens(30, 90));
        assert!(!shortens(30, 0));
        assert!(!shortens(30, 30));
    }
}
