//! A server's own emoji in the window (specs/emoji.md 10, and the step 12 boards): the
//! picker's server section, reacting with one, the composer's emoji button and its `:` list,
//! how a body and a reaction chip draw one, and Server settings' Emoji section (the upload
//! form, renaming in place, removing after asking).
//!
//! The rules are the core's: which emoji a server has, its limit and whether this account
//! may manage them (the snapshot), the matches for what follows a `:` (`App::emoji_matches`),
//! the name a file's name makes (`lumia_core::emoji::name_from_file`) and whether a name is
//! one (`is_name`), the refs a send makes and the grouping of reactions. This holds what the
//! window shows of them and every sentence the boards say.
//!
//! None of it is offered in a personal conversation (spec 1): the picker has no server
//! section there, "Frequently used" none of a server's, the `:` list lists nothing, and the
//! words `:name:` are text.
//!
//! The pictures are the session's (`avatars`), decoded as avatars are (`pictures`). Rune
//! plays an animated GIF wherever it draws one as an element's pixels (the picker, the `:`
//! list, a chip, settings), and draws the first frame of one set in text (a body; see
//! `ui::body`). Under the system's Reduce Motion the window draws an animated one's first
//! frame everywhere, which Rune does not hand an app: the decoded first frame is made here
//! from the GIF itself, once ([`Stills`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumia_client::app::{EmojiRefSummary, EmojiSummary, ServerSummary};
use rune::{FileDialogOptions, Image, Key, KeyEvent, TextEdit, Tx};

use super::{Popover, State};
use crate::session::Failure;
use crate::session::emoji::EmojiUpdate;
use crate::session::media::size_words;
use crate::ui::body::{BodyEmoji, EmojiPicture};

/// How "Frequently used" keeps one of a server's emoji, among the Unicode ones it keeps as
/// themselves: by id, so a rename keeps it and a removal drops it.
const CUSTOM: &str = "custom:";

/// How many rows the `:` list shows.
const LIST_ROWS: usize = 8;

/// The words a name is refused with before it is sent, and when the instance refuses it.
pub const NAME_RULE: &str = "A name is 2 to 32 of a to z, 0 to 9 and _, with no spaces.";

/// Server settings' Emoji section, its forms and its question.
#[derive(Debug, Default)]
pub struct EmojiForms {
    /// The upload form, once a picture is chosen.
    pub upload: Option<Upload>,
    /// The row being renamed in place.
    pub renaming: Option<Rename>,
    /// The emoji "Remove :name:?" asks about, the button it hangs from, and why a removal
    /// was refused.
    pub removing: Option<Removing>,
    /// Why the system's file dialog did not open, or a file could not be read before a form
    /// was there to say so.
    pub error: String,
    /// The file asked to be read for the form, until it is: an answer for another is not
    /// taken.
    reading: Option<PathBuf>,
}

/// Upload emoji's form: the picture chosen, its name to change, and why either was refused.
#[derive(Debug)]
pub struct Upload {
    pub file: String,
    pub size: u64,
    pub bytes: Arc<[u8]>,
    pub media_type: &'static str,
    /// The chosen file decoded to be drawn as it was chosen: this computer's own file, and
    /// never sent anywhere decoded.
    pub picture: Option<Image>,
    pub name: TextEdit,
    pub name_error: String,
    pub picture_error: String,
    pub busy: bool,
}

/// A row renamed in place.
#[derive(Debug)]
pub struct Rename {
    pub emoji_id: String,
    /// Its name when Rename was pressed: "New name for :was:".
    pub was: String,
    pub name: TextEdit,
    pub error: String,
    pub busy: bool,
}

#[derive(Debug)]
pub struct Removing {
    pub emoji_id: String,
    pub name: String,
    pub anchor: String,
    pub error: String,
    pub busy: bool,
}

/// A `:name` being typed at the caret, which the `:` list completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColonQuery {
    /// The byte of its `:` and the caret.
    pub start: usize,
    pub end: usize,
    /// What follows the `:`.
    pub query: String,
    /// The row Up and Down have moved to.
    pub chosen: usize,
}

/// The `:name` ending at the caret of `draft`: a `:` at the start or after whitespace, and
/// what can be in a name after it (letters, digits and `_`). None while a selection is made.
/// The core decides what it matches, and lists nothing for fewer than two characters, so a
/// time like 10:30 or a `:)` lists nothing.
pub fn colon_at(draft: &TextEdit) -> Option<ColonQuery> {
    let (from, to) = draft.range();
    if from != to {
        return None;
    }
    let before = draft.text.get(..to)?;
    let start = before.rfind(':')?;
    let query = &before[start + 1..];
    let named = query.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let opens = before[..start]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    (named && opens).then(|| ColonQuery {
        start,
        end: to,
        query: query.to_string(),
        chosen: 0,
    })
}

/// What a refusal of an upload or a rename says, in the boards' words, and whether it is
/// about the name (said under the field) or the picture (said under the picture).
pub fn refusal(server: &str, name: &str, failure: &Failure) -> (bool, String) {
    let field = failure.details.get("field").map(String::as_str);
    let reason = failure.details.get("reason").map(String::as_str);
    match (failure.code.as_str(), field, reason) {
        ("ALREADY_EXISTS", ..) => (
            true,
            format!("{server} already has :{name}:. Choose another name."),
        ),
        ("INVALID_ARGUMENT", Some("name"), _) => (true, NAME_RULE.into()),
        ("INVALID_ARGUMENT", _, Some("too_large")) => (
            false,
            "This picture is too large for an emoji. An animated one can have at most 200 \
             frames and come to 512 KB once made."
                .into(),
        ),
        ("INVALID_ARGUMENT", _, Some("unsupported")) => (
            false,
            "Lumia can’t use this file. Choose a PNG, JPEG, WebP or GIF.".into(),
        ),
        ("INVALID_ARGUMENT", _, Some("unreadable")) => (
            false,
            format!("That file could not be read: {}.", failure.message),
        ),
        ("INVALID_ARGUMENT", ..) => (false, "This picture couldn’t be read.".into()),
        ("RESOURCE_EXHAUSTED", ..) => (false, full_words(server, limit_of(failure))),
        ("PERMISSION_DENIED", ..) => (false, "You can’t manage this server’s emoji.".into()),
        ("NOT_FOUND", ..) => (false, "That emoji was removed.".into()),
        ("FAILED_PRECONDITION", ..) => (false, "This instance does not keep pictures.".into()),
        _ => (false, failure.message.clone()),
    }
}

fn limit_of(failure: &Failure) -> Option<u32> {
    failure.details.get("limit")?.parse().ok()
}

/// What the section says at its limit: "Acme has as many emoji as it may (50). Remove one to
/// upload another."
pub fn full_words(server: &str, limit: Option<u32>) -> String {
    match limit {
        Some(limit) => {
            format!("{server} has as many emoji as it may ({limit}). Remove one to upload another.")
        }
        None => format!("{server} has as many emoji as it may. Remove one to upload another."),
    }
}

/// The section's subtitle below its limit.
pub fn about_words(server: &str) -> String {
    format!(
        "Used as :name: in {server}’s channels and server DMs, and as reactions. A PNG, JPEG, \
         WebP or GIF up to 8 MB, made to fit 128 pixels; an animated one plays."
    )
}

/// The count against the limit: "12 of 50".
pub fn count_words(server: &ServerSummary) -> String {
    format!("{} of {}", server.emoji.len(), server.emoji_limit)
}

/// Whether the server has as many as it may (or more, where its limit was lowered).
pub fn at_limit(server: &ServerSummary) -> bool {
    server.emoji_limit > 0 && server.emoji.len() >= server.emoji_limit as usize
}

/// "Remove :name:?" and its line.
pub fn remove_title(name: &str) -> String {
    format!("Remove :{name}:?")
}
pub const REMOVE_LINE: &str = "Messages that used it will show its name instead.";

/// An emoji's shape, its width over its height.
pub fn aspect(e: &EmojiSummary) -> f32 {
    if e.width == 0 || e.height == 0 {
        1.0
    } else {
        e.width as f32 / e.height as f32
    }
}

/// The first frames of animated emoji, for Reduce Motion: decoded from the GIF itself, once
/// a picture, and handed to Rune as a still.
#[derive(Default)]
pub struct Stills {
    images: HashMap<String, Option<Image>>,
}

impl std::fmt::Debug for Stills {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stills")
            .field("images", &self.images.len())
            .finish()
    }
}

/// The first frame of a GIF as straight RGBA, or none when it is not one that decodes.
fn first_frame(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let frame = decoder.into_frames().next()?.ok()?;
    let buffer = frame.into_buffer();
    Some((buffer.width(), buffer.height(), buffer.into_raw()))
}

impl State {
    // ---- Drawing ---------------------------------------------------------------------

    /// The picture to draw for one of a server's emoji, when its bytes are held and decoded
    /// enough to draw: under Reduce Motion an animated one's first frame.
    pub fn emoji_image(&self, emoji: &EmojiSummary, still: bool) -> Option<&Image> {
        if still && emoji.animated {
            return self.stills.images.get(&emoji.image_url)?.as_ref();
        }
        self.picture_at(&emoji.image_url)
    }

    /// Makes the first frames Reduce Motion draws of the animated emoji whose pictures are
    /// held, once each. Called before a frame, never in a view.
    pub(super) fn sync_stills(&mut self) -> bool {
        // One no server lists any more (removed) is let go, as its picture is.
        let listed: std::collections::HashSet<&str> = self
            .session
            .servers()
            .iter()
            .flat_map(|s| s.emoji.iter())
            .map(|e| e.image_url.as_str())
            .collect();
        self.stills
            .images
            .retain(|url, _| listed.contains(url.as_str()));
        let wanted: Vec<(String, Arc<[u8]>)> = self
            .session
            .servers()
            .iter()
            .flat_map(|s| s.emoji.iter())
            .filter(|e| e.animated && !self.stills.images.contains_key(&e.image_url))
            .filter_map(|e| {
                let bytes = self.session.avatars.bytes_of(&e.image_url)?;
                Some((e.image_url.clone(), Arc::clone(bytes)))
            })
            .collect();
        let made = !wanted.is_empty();
        for (url, bytes) in wanted {
            let image = first_frame(&bytes).map(|(w, h, rgba)| self.pictures.rgba(w, h, rgba));
            self.stills.images.insert(url, image);
        }
        made
    }

    /// How a body draws a ref to `emoji_id` in `server_id`'s conversation: its picture
    /// when this computer holds the emoji, none (its text) when it does not.
    pub fn body_picture(&self, server_id: &str, emoji_id: &str) -> Option<EmojiPicture> {
        let e = self.session.emoji_in(server_id, emoji_id)?;
        Some(EmojiPicture {
            // Its room is kept until it has decoded.
            pixels: self
                .picture_at(&e.image_url)
                .map(|image| image.pixels().clone())
                .unwrap_or_default(),
            aspect: aspect(e),
            name: e.name.clone(),
        })
    }

    /// A body's refs as drawn where the body is drawn as sent (a Mentions page): each over
    /// its `:name:`, in `server_id`'s conversation.
    pub fn body_emoji(
        &self,
        server_id: &str,
        body: &str,
        refs: &[EmojiRefSummary],
    ) -> Vec<BodyEmoji> {
        refs.iter()
            .filter(|r| r.start < r.end && body.get(r.start as usize..r.end as usize).is_some())
            .map(|r| BodyEmoji {
                range: r.start as usize..r.end as usize,
                picture: self.body_picture(server_id, &r.emoji_id),
            })
            .collect()
    }

    // ---- The picker -------------------------------------------------------------------

    /// The composer's emoji button: the picker, hung from it, putting what is chosen in at
    /// the caret and staying open for more until Escape or a press outside.
    pub fn open_composer_picker(&mut self) {
        self.replies.picker = Some(super::replies::Picker {
            message_id: String::new(),
            anchor: COMPOSER_EMOJI.into(),
            search: TextEdit::new(""),
            composer: true,
        });
        self.open_popover(Popover::EmojiPicker);
    }

    /// Whether the picker open is the composer's.
    pub fn composer_picker_open(&self) -> bool {
        self.popover == Some(Popover::EmojiPicker)
            && self.replies.picker.as_ref().is_some_and(|p| p.composer)
    }

    /// Escape, or a press outside the composer's picker: it goes, and the caret is the
    /// field's.
    pub fn close_composer_picker(&mut self, tx: &mut Tx) {
        self.replies.picker = None;
        self.close_popover();
        tx.focus("composer", false);
    }

    /// One of the server's emoji chosen in the picker: a reaction with it to the message the
    /// picker is for, or its `:name:` and a space at the composer's caret.
    pub fn pick_custom(&mut self, emoji_id: &str) {
        let Some(server) = self.session.emoji_server() else {
            return;
        };
        let Some(emoji) = server
            .emoji
            .iter()
            .find(|e| e.emoji_id == emoji_id)
            .cloned()
        else {
            return;
        };
        let composer = self.replies.picker.as_ref().is_some_and(|p| p.composer);
        if composer {
            self.insert_in_composer(&format!(":{}: ", emoji.name));
            self.keep_recent(&format!("{CUSTOM}{emoji_id}"));
            return;
        }
        if let Some(picker) = self.replies.picker.take() {
            self.react_custom(&picker.message_id, emoji_id);
        }
        self.close_popover();
    }

    /// Puts `words` in at the composer's caret, over what is selected. The picker's cell
    /// gives the field the caret back (`ui::emoji`).
    pub(super) fn insert_in_composer(&mut self, words: &str) {
        self.draft.insert(words);
        self.follow_mention();
        self.follow_colon();
    }

    /// A reaction with one of the server's emoji, from the picker or a chip: added, or taken
    /// away when this account has it. Kept in "Frequently used".
    pub fn react_custom(&mut self, message_id: &str, emoji_id: &str) {
        self.session.react_custom(message_id, emoji_id);
        self.keep_recent(&format!("{CUSTOM}{emoji_id}"));
    }

    /// Puts one first in "Frequently used".
    pub(super) fn keep_recent(&mut self, kept: &str) {
        let recent = &mut self.settings.recent_emoji;
        recent.retain(|e| e != kept);
        recent.insert(0, kept.to_string());
        recent.truncate(super::replies::RECENT);
        self.settings.save(&self.data_dir);
    }

    /// The server's emoji "Frequently used" holds here: those this computer used that are
    /// live emoji of the server on screen, by id; none in a personal conversation.
    pub fn frequent_custom(&self, kept: &str) -> Option<EmojiSummary> {
        let id = kept.strip_prefix(CUSTOM)?;
        let server = self.session.emoji_server()?;
        server.emoji.iter().find(|e| e.emoji_id == id).cloned()
    }

    /// The server's emoji a picker's search finds: those whose name begins with it, then
    /// those that hold it (a leading or trailing `:` set aside), as the web's picker finds
    /// them. Not the `:` list's rule, which lists nothing for one letter: a search here does.
    pub fn custom_found(&self, wanted: &str) -> Vec<EmojiSummary> {
        let Some(server) = self.session.emoji_server() else {
            return Vec::new();
        };
        let wanted = wanted.trim().trim_matches(':').to_lowercase();
        if wanted.is_empty() {
            return Vec::new();
        }
        let names = server.emoji.iter().map(|e| e.name.as_str());
        let begins = names.clone().filter(|n| n.starts_with(&wanted));
        let holds = names.filter(|n| !n.starts_with(&wanted) && n.contains(&wanted));
        begins
            .chain(holds)
            .filter_map(|name| server.emoji.iter().find(|e| e.name == name).cloned())
            .collect()
    }

    // ---- The `:` list ------------------------------------------------------------------

    /// The `:` list's rows for what is being typed: the server's emoji the core matches
    /// (`lumia_core::emoji::matches`, as `App::emoji_matches` reads them, over the emoji the
    /// session holds), in its order. None outside a server's conversation.
    pub fn colon_choices(&self) -> Vec<EmojiSummary> {
        let (Some(query), Some(server)) = (&self.replies.colon, self.session.emoji_server()) else {
            return Vec::new();
        };
        let names = server.emoji.iter().map(|e| e.name.as_str());
        lumia_core::emoji::matches(&query.query, names)
            .into_iter()
            .filter_map(|name| server.emoji.iter().find(|e| e.name == name).cloned())
            .take(LIST_ROWS)
            .collect()
    }

    /// The draft changed: what the `:` list completes follows the caret.
    pub fn follow_colon(&mut self) {
        let found = colon_at(&self.draft);
        self.replies.colon = match (found, self.replies.colon.take()) {
            (Some(mut found), Some(was)) if found.start == was.start => {
                if found.query == was.query {
                    found.chosen = was.chosen;
                }
                Some(found)
            }
            (found, _) => found,
        };
    }

    /// Picks row `index` of the `:` list: `:name` becomes `:name: `.
    pub fn pick_colon(&mut self, index: usize) {
        let choices = self.colon_choices();
        let (Some(query), Some(choice)) =
            (self.replies.colon.take(), choices.into_iter().nth(index))
        else {
            return;
        };
        let mut text = self.draft.text.clone();
        let words = format!(":{}: ", choice.name);
        text.replace_range(query.start..query.end, &words);
        self.draft.replace_text(text);
        self.draft.set_caret(query.start + words.len());
    }

    /// The `:` list's keys while it shows: Up, Down, Return or Tab, Escape. True when taken.
    pub(super) fn colon_key(&mut self, key: &KeyEvent) -> bool {
        let rows = self.colon_choices().len();
        if rows == 0 {
            return false;
        }
        let Some(query) = &mut self.replies.colon else {
            return false;
        };
        match key.key {
            Key::Up => query.chosen = (query.chosen + rows - 1) % rows,
            Key::Down => query.chosen = (query.chosen + 1) % rows,
            Key::Enter | Key::Tab if !key.mods.shift => {
                let chosen = query.chosen.min(rows - 1);
                self.pick_colon(chosen);
            }
            Key::Escape => self.replies.colon = None,
            _ => return false,
        }
        true
    }

    // ---- Server settings' Emoji section ------------------------------------------------

    /// The server whose settings are open, when this account may manage its emoji.
    fn emoji_settings_server(&self) -> Option<ServerSummary> {
        let id = self.settings_server()?;
        self.session
            .server(&id)
            .filter(|s| s.can_manage_emoji)
            .cloned()
    }

    /// Upload emoji, and the form's "Choose another…": the system's file dialog, the
    /// picture chosen read for the form.
    pub fn choose_emoji_file(&mut self, tx: &mut Tx) {
        if self.emoji.upload.as_ref().is_some_and(|u| u.busy) {
            return;
        }
        let options = FileDialogOptions {
            title: "Choose a picture".into(),
            ..FileDialogOptions::default()
        }
        .filter("Pictures", &["png", "jpg", "jpeg", "webp", "gif"]);
        tx.open_file(options, |s: &mut State, answer, _| match answer {
            Ok(Some(path)) => s.read_emoji_file(&path),
            Ok(None) => {}
            Err(error) => s.emoji.error = format!("The file dialog could not open: {error}"),
        });
    }

    /// Reads the file at `path` for the upload form.
    pub fn read_emoji_file(&mut self, path: &Path) {
        self.emoji.error.clear();
        self.emoji.reading = Some(path.to_path_buf());
        self.session.read_emoji_file(path);
    }

    /// A file read for the upload form: the form, with the picture as chosen and the name
    /// the core makes of the file's name.
    fn emoji_file_read(
        &mut self,
        path: &Path,
        result: &Result<(Arc<[u8]>, &'static str), Failure>,
    ) {
        if self.emoji.reading.as_deref() != Some(path) {
            return;
        }
        self.emoji.reading = None;
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let server = self
            .emoji_settings_server()
            .map(|s| s.name)
            .unwrap_or_default();
        let (bytes, media_type, picture_error) = match result {
            Ok((bytes, media_type)) => (Arc::clone(bytes), *media_type, String::new()),
            Err(failure) => (Arc::from(&[][..]), "", refusal(&server, "", failure).1),
        };
        let picture = (!bytes.is_empty()).then(|| self.pictures.decode(Arc::clone(&bytes)));
        let name = lumia_core::emoji::name_from_file(&file);
        let mut field = TextEdit::new(&name);
        field.set_caret(name.len());
        self.emoji.upload = Some(Upload {
            file,
            size: bytes.len() as u64,
            bytes,
            media_type,
            picture,
            name: field,
            name_error: String::new(),
            picture_error,
            busy: false,
        });
    }

    /// The upload form's size line: "38 KB".
    pub fn upload_size(upload: &Upload) -> String {
        size_words(upload.size)
    }

    /// The upload form's Save.
    pub fn save_upload(&mut self) {
        let Some(server) = self.emoji_settings_server() else {
            return;
        };
        let Some(upload) = &mut self.emoji.upload else {
            return;
        };
        if upload.busy || upload.bytes.is_empty() {
            return;
        }
        let name = upload.name.text.trim().to_lowercase();
        if !lumia_core::emoji::is_name(&name) {
            upload.name_error = NAME_RULE.into();
            return;
        }
        upload.busy = true;
        upload.name_error.clear();
        upload.picture_error.clear();
        let (bytes, media_type) = (Arc::clone(&upload.bytes), upload.media_type);
        self.session
            .upload_emoji(&server.server_id, &name, bytes, media_type);
    }

    pub fn cancel_upload(&mut self) {
        self.emoji.upload = None;
    }

    /// A row's Rename: the row becomes its field, holding the name (the view puts the caret
    /// there).
    pub fn begin_rename(&mut self, emoji_id: &str) {
        let Some(server) = self.emoji_settings_server() else {
            return;
        };
        let Some(emoji) = server.emoji.iter().find(|e| e.emoji_id == emoji_id) else {
            return;
        };
        let mut name = TextEdit::new(&emoji.name);
        name.set_caret(emoji.name.len());
        self.emoji.renaming = Some(Rename {
            emoji_id: emoji_id.to_string(),
            was: emoji.name.clone(),
            name,
            error: String::new(),
            busy: false,
        });
    }

    pub fn save_rename(&mut self) {
        let Some(server) = self.emoji_settings_server() else {
            return;
        };
        let Some(form) = &mut self.emoji.renaming else {
            return;
        };
        let name = form.name.text.trim().to_lowercase();
        if form.busy || name.is_empty() {
            return;
        }
        if name == form.was {
            self.emoji.renaming = None;
            return;
        }
        if !lumia_core::emoji::is_name(&name) {
            form.error = NAME_RULE.into();
            return;
        }
        form.busy = true;
        form.error.clear();
        let emoji_id = form.emoji_id.clone();
        self.session
            .rename_emoji(&server.server_id, &emoji_id, &name);
    }

    pub fn cancel_rename(&mut self) {
        if !self.emoji.renaming.as_ref().is_some_and(|r| r.busy) {
            self.emoji.renaming = None;
        }
    }

    /// The field being renamed, for its `on_edit`.
    pub fn rename_field(&mut self) -> &mut TextEdit {
        match &mut self.emoji.renaming {
            Some(form) => &mut form.name,
            // Gone between the frame and the edit: an edit to nothing kept.
            None => &mut self.member_filter,
        }
    }

    /// The upload form's name field, for its `on_edit`.
    pub fn upload_field(&mut self) -> &mut TextEdit {
        match &mut self.emoji.upload {
            Some(form) => &mut form.name,
            None => &mut self.member_filter,
        }
    }

    /// A row's Remove: asked first, against the button.
    pub fn ask_remove_emoji(&mut self, emoji_id: &str, anchor: &str) {
        let Some(server) = self.emoji_settings_server() else {
            return;
        };
        let Some(emoji) = server.emoji.iter().find(|e| e.emoji_id == emoji_id) else {
            return;
        };
        self.emoji.removing = Some(Removing {
            emoji_id: emoji_id.to_string(),
            name: emoji.name.clone(),
            anchor: anchor.to_string(),
            error: String::new(),
            busy: false,
        });
        self.open_popover(Popover::RemoveEmoji);
    }

    pub fn confirm_remove_emoji(&mut self) {
        let Some(server) = self.emoji_settings_server() else {
            return;
        };
        let Some(form) = &mut self.emoji.removing else {
            return;
        };
        if form.busy {
            return;
        }
        form.busy = true;
        form.error.clear();
        let emoji_id = form.emoji_id.clone();
        self.session.remove_emoji(&server.server_id, &emoji_id);
    }

    /// What an emoji call came to, for the form that asked.
    pub(super) fn react_emoji(&mut self, update: &EmojiUpdate) {
        let server_name = |s: &State, id: &str| {
            s.session
                .server(id)
                .map(|s| s.name.clone())
                .unwrap_or_default()
        };
        match update {
            EmojiUpdate::Read { path, result } => self.emoji_file_read(path, result),
            EmojiUpdate::Uploaded { server_id, result } => {
                let name = server_name(self, server_id);
                let Some(upload) = &mut self.emoji.upload else {
                    return;
                };
                upload.busy = false;
                match result {
                    Ok(_) => self.emoji.upload = None,
                    Err(failure) => {
                        let typed = upload.name.text.trim().to_lowercase();
                        let (about_name, words) = refusal(&name, &typed, failure);
                        if about_name {
                            upload.name_error = words;
                        } else {
                            upload.picture_error = words;
                        }
                    }
                }
            }
            EmojiUpdate::Renamed {
                server_id,
                emoji_id,
                result,
            } => {
                let name = server_name(self, server_id);
                let Some(form) = self
                    .emoji
                    .renaming
                    .as_mut()
                    .filter(|f| f.emoji_id == *emoji_id)
                else {
                    return;
                };
                form.busy = false;
                match result {
                    Ok(_) => self.emoji.renaming = None,
                    Err(failure) => {
                        let typed = form.name.text.trim().to_lowercase();
                        form.error = refusal(&name, &typed, failure).1;
                    }
                }
            }
            EmojiUpdate::Removed {
                server_id,
                emoji_id,
                result,
            } => {
                let name = server_name(self, server_id);
                let Some(form) = self
                    .emoji
                    .removing
                    .as_mut()
                    .filter(|f| f.emoji_id == *emoji_id)
                else {
                    return;
                };
                form.busy = false;
                match result {
                    Ok(_) => {
                        self.emoji.removing = None;
                        if self.popover == Some(Popover::RemoveEmoji) {
                            self.close_popover();
                        }
                    }
                    Err(failure) => form.error = refusal(&name, "", failure).1,
                }
            }
        }
    }

    /// After anything arrived: a rename or a question about an emoji removed elsewhere goes,
    /// and the forms go when the section is no longer this account's.
    pub(super) fn follow_emoji(&mut self) {
        let Some(server) = self.emoji_settings_server() else {
            if self.emoji.upload.is_some()
                || self.emoji.renaming.is_some()
                || self.emoji.removing.is_some()
            {
                self.emoji = EmojiForms::default();
                if self.popover == Some(Popover::RemoveEmoji) {
                    self.close_popover();
                }
            }
            return;
        };
        let live = |id: &str| server.emoji.iter().any(|e| e.emoji_id == id);
        if self
            .emoji
            .renaming
            .as_ref()
            .is_some_and(|f| !live(&f.emoji_id))
        {
            self.emoji.renaming = None;
        }
        if self
            .emoji
            .removing
            .as_ref()
            .is_some_and(|f| !live(&f.emoji_id) && !f.busy)
        {
            self.emoji.removing = None;
            if self.popover == Some(Popover::RemoveEmoji) {
                self.close_popover();
            }
        }
    }
}

/// The key of the composer's emoji button, which its picker hangs from.
pub const COMPOSER_EMOJI: &str = "composer-emoji";

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> TextEdit {
        let mut field = TextEdit::new(text);
        field.set_caret(text.len());
        field
    }

    #[test]
    fn a_colon_opens_a_name_only_at_the_start_or_after_a_space() {
        let found = colon_at(&typed("Shipped the importer :par"));
        assert_eq!(
            found.map(|f| (f.start, f.query)),
            Some((21, "par".to_string()))
        );
        assert_eq!(colon_at(&typed(":p")).map(|f| f.query), Some("p".into()));
        // A time, a face, a word with a colon in it, a name already closed: nothing opens.
        for quiet in ["at 10:30", "ok :)", "see:par", "done: ", ":par ty", ":par:"] {
            assert!(colon_at(&typed(quiet)).is_none(), "{quiet}");
        }
        // A selection is not a caret.
        let mut field = typed("hi :par");
        field.set_selection(3, 7);
        assert!(colon_at(&field).is_none());
    }

    fn failure(code: &str, details: &[(&str, &str)]) -> Failure {
        Failure {
            code: code.into(),
            message: "said".into(),
            details: details
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn refusals_are_said_in_the_boards_words() {
        assert_eq!(
            refusal("Acme", "partyparrot", &failure("ALREADY_EXISTS", &[])),
            (
                true,
                "Acme already has :partyparrot:. Choose another name.".into()
            )
        );
        let name = failure("INVALID_ARGUMENT", &[("field", "name")]);
        assert_eq!(refusal("Acme", "x", &name), (true, NAME_RULE.into()));
        let picture = |reason| {
            let failure = failure(
                "INVALID_ARGUMENT",
                &[("field", "picture"), ("reason", reason)],
            );
            refusal("Acme", "x", &failure)
        };
        assert_eq!(
            picture("too_large"),
            (
                false,
                "This picture is too large for an emoji. An animated one can have at most 200 \
                 frames and come to 512 KB once made."
                    .into()
            )
        );
        assert_eq!(
            picture("unsupported").1,
            "Lumia can’t use this file. Choose a PNG, JPEG, WebP or GIF."
        );
        assert_eq!(picture("invalid").1, "This picture couldn’t be read.");
        let full = failure("RESOURCE_EXHAUSTED", &[("state", "emoji"), ("limit", "50")]);
        assert_eq!(
            refusal("Acme", "x", &full),
            (
                false,
                "Acme has as many emoji as it may (50). Remove one to upload another.".into()
            )
        );
    }

    #[test]
    fn the_still_of_an_animated_emoji_is_its_first_frame() {
        let gif = include_bytes!("../scenes/emoji/partyparrot.gif");
        let (w, h, rgba) = first_frame(gif).unwrap_or_default();
        assert_eq!((w, h, rgba.len()), (128, 128, 128 * 128 * 4));
        assert!(first_frame(include_bytes!("../scenes/emoji/acme.png")).is_none());
    }
}
