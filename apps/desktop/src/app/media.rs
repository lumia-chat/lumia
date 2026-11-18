//! Attachments and link cards in the window (specs/media.md 5 and 6): the pictures the
//! session holds decoded for drawing, the composer's ways in (the button and the system's
//! file dialog, a paste, a drop onto the conversation), the alt text of what waits, when the
//! links typed are previewed, the full-screen viewer, saving, the system's player, and what
//! an edit takes out of a message.
//!
//! A picture is decoded on Rune's image cache's worker, never in a view or a frame, as an
//! avatar is (`pictures`); an image's display picture arrives from the session already
//! decoded and made small (`session::media`), and only the viewer holds it whole. A placeholder is a few dozen pixels from the core
//! (`App::placeholder_image`), made once per attachment.
//!
//! Rune draws an animated GIF in motion by itself. Under the system's Reduce Motion the
//! timeline draws its first frame (its thumbnail) with a GIF badge instead, and a click
//! plays it ([`State::play_gif`]).
//!
//! Rune has a video player on macOS and Windows and none on Linux. Where it has one, a video
//! or a recording plays in the conversation (`playback`). On Linux it opens in the system's
//! own player: the core opens it, it is written to the temporary directory under a name its
//! media type gives, and the platform opens that file (`platform::settle`). A video's poster
//! is drawn when its sender's platform made one; this shell makes none when it sends one.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use lumia_client::app::{App, AttachmentSummary, BlobSummary};
use rune::{
    ClipboardContent, Drag, FileDialogOptions, Image, ImageCache, Key, KeyEvent, TextEdit, Tx,
    Waker,
};

use super::State;
use crate::session::Session;
use crate::session::Update;
use crate::session::media::{MediaUpdate, Picture, Stage, small, small_key};
use crate::settings::Media;

/// How long the words must rest before their links are previewed (spec 6.4).
pub const PREVIEW_AFTER_MICROS: i64 = 500_000;

/// The viewer's zoom: from fitting the window to eight times that, a step at a time.
pub const ZOOM_MIN: f32 = 1.0;
pub const ZOOM_MAX: f32 = 8.0;
const ZOOM_STEP: f32 = 1.5;

/// The picture on the full-screen viewer, and how it is zoomed and moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Viewer {
    pub attachment: AttachmentSummary,
    /// 1 fits the window.
    pub zoom: f32,
    /// How far the picture is moved from the centre, in points.
    pub pan: [f32; 2],
    /// Where a drag began.
    grab: [f32; 2],
}

/// The window's side of attachments.
#[derive(Default)]
pub struct MediaUi {
    /// None until the window is set up or a picture is first wanted.
    cache: Option<ImageCache>,
    /// The pictures held, by the session's key.
    images: HashMap<String, Image>,
    /// Placeholders, by attachment or draft id.
    placeholders: HashMap<String, Image>,
    /// The session's revisions these were made from.
    seen: Option<(u64, u64)>,
    /// The alt text fields of the pictures in the composer's strip, by their number there.
    pub alts: HashMap<u64, TextEdit>,
    pub viewer: Option<Viewer>,
    /// GIFs played by a click under Reduce Motion, by attachment id.
    pub playing: HashSet<String>,
    /// When the draft last changed and to what, until its links have been looked at.
    typed: Option<(String, i64)>,
    /// What the draft held when its links were last looked at.
    previewed: String,
    /// Files written for the system's player, for the platform to open.
    pub to_open: Vec<PathBuf>,
    /// Files are being dragged over the conversation.
    pub dropping: bool,
    /// While a message is edited: the attachments and the links' cards its × took out.
    pub edit_removed: Vec<String>,
    pub edit_dropped: Vec<String>,
}

impl std::fmt::Debug for MediaUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaUi")
            .field("images", &self.images.len())
            .field("viewer", &self.viewer.is_some())
            .finish_non_exhaustive()
    }
}

impl MediaUi {
    /// Starts decoding; `waker` is the window's, called when a picture is ready.
    pub fn start(&mut self, waker: Waker) {
        self.cache = Some(ImageCache::new(waker));
        self.seen = None;
    }

    fn cache(&mut self) -> &ImageCache {
        self.cache
            .get_or_insert_with(|| ImageCache::new(Arc::new(|| {})))
    }

    /// Brings the pictures in line with what the session holds; true when that changed which
    /// there are.
    pub fn sync(&mut self, session: &Session) -> bool {
        let now = (session.revision, session.media.revision);
        if self.seen == Some(now) {
            return false;
        }
        self.seen = Some(now);
        let mut images = HashMap::new();
        for (key, picture) in session.media.held() {
            let image = match (self.images.remove(key), picture) {
                (Some(image), _) => image,
                (None, Picture::Bytes(bytes)) => self.cache().load(Arc::clone(bytes)),
                (
                    None,
                    Picture::Small {
                        width,
                        height,
                        rgba,
                    },
                ) => self.cache().rgba(*width, *height, Arc::clone(rgba)),
            };
            images.insert(key.to_string(), image);
        }
        let mut placeholders = HashMap::new();
        let shown = session
            .entries
            .iter()
            .flat_map(|e| e.attachments.iter())
            .map(|a| (a.attachment_id.as_str(), a.placeholder.as_slice()));
        let waiting = session
            .media
            .waiting
            .iter()
            .filter_map(|w| w.draft())
            .map(|d| (d.draft_id.as_str(), d.placeholder.as_slice()));
        let viewed = self.viewer.as_ref().map(|v| {
            (
                v.attachment.attachment_id.as_str(),
                v.attachment.placeholder.as_slice(),
            )
        });
        let wanted: Vec<(String, Vec<u8>)> = shown
            .chain(waiting)
            .chain(viewed)
            .filter(|(_, p)| !p.is_empty())
            .map(|(id, p)| (id.to_string(), p.to_vec()))
            .collect();
        for (id, placeholder) in wanted {
            if placeholders.contains_key(&id) {
                continue;
            }
            let image = match self.placeholders.remove(&id) {
                Some(image) => Some(image),
                None => App::placeholder_image(&placeholder)
                    .map(|p| self.cache().rgba(p.width, p.height, p.rgba)),
            };
            if let Some(image) = image {
                placeholders.insert(id, image);
            }
        }
        let changed = self.images.len() != images.len()
            || !self.images.is_empty()
            || !images.is_empty()
            || !placeholders.is_empty();
        self.images = images;
        self.placeholders = placeholders;
        changed
    }

    /// Waits for every picture to finish decoding, for a scene drawn in one frame.
    pub fn wait(&self) {
        for image in self.images.values().chain(self.placeholders.values()) {
            let _ = image.wait();
        }
    }

    /// The picture held under the session's `key`.
    pub fn image(&self, key: &str) -> Option<&Image> {
        self.images.get(key)
    }

    /// The placeholder of an attachment or a draft.
    pub fn placeholder(&self, id: &str) -> Option<&Image> {
        self.placeholders.get(id)
    }

    /// The alt text field of a picture in the strip.
    pub fn alt(&self, local: u64) -> Option<&TextEdit> {
        self.alts.get(&local)
    }
}

/// The size a picture `width` by `height` is drawn at in a message: its own, made smaller to
/// fit `max_width` and `max_height` (spec 5: at most 400 tall), never larger. An unknown size
/// is drawn as a square of the largest room.
pub fn fitted(width: u32, height: u32, max_width: f32, max_height: f32) -> [f32; 2] {
    if width == 0 || height == 0 {
        let side = max_width.min(max_height);
        return [side, side];
    }
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (width as f32, height as f32);
    let k = (max_width / w).min(max_height / h).min(1.0);
    [(w * k).round().max(1.0), (h * k).round().max(1.0)]
}

/// How several pictures of one message are laid out: in columns of equal squares, two
/// across up to four pictures and three for more, each `cell` points a side, no wider than
/// `max_width` together. Answers the number of columns and the cell's side.
pub fn grid(count: usize, max_width: f32, cell: f32, gap: f32) -> (usize, f32) {
    let columns = if count <= 4 { 2 } else { 3 };
    #[allow(clippy::cast_precision_loss)]
    let across = columns as f32;
    let side = ((max_width - gap * (across - 1.0)) / across)
        .min(cell)
        .max(1.0);
    (columns, side.floor())
}

impl State {
    // ---- The composer ------------------------------------------------------------------

    /// The attach button: the system's file dialog, then the file chosen is attached.
    pub fn choose_attachment(&mut self, tx: &mut Tx) {
        if !self.session.can_attach() {
            return;
        }
        let options = FileDialogOptions {
            title: "Attach a file".into(),
            ..FileDialogOptions::default()
        };
        tx.open_file(options, |s: &mut State, answer, _| match answer {
            Ok(Some(path)) => s.session.attach_path(&path),
            Ok(None) => {}
            Err(error) => {
                s.session.last_error = Some(format!("The file dialog could not open: {error}"));
            }
        });
    }

    /// A paste into the composer: files copied from a file manager are attached, and so is
    /// a picture with no words beside it (a screenshot). True when it was taken; words paste
    /// as words.
    pub fn composer_paste(&mut self, content: &ClipboardContent) -> bool {
        if !self.session.can_attach() {
            return false;
        }
        if !content.files().is_empty() {
            for file in content.files() {
                self.session.attach_path(file);
            }
            return true;
        }
        match content.image() {
            Some(image) if content.text().trim().is_empty() => {
                self.session
                    .attach_bytes("Pasted image.png", image.png().to_vec());
                true
            }
            _ => false,
        }
    }

    /// Files dropped onto the conversation.
    pub fn drop_files(&mut self, files: &[PathBuf]) {
        self.media.dropping = false;
        if !self.session.can_attach() {
            return;
        }
        for file in files {
            self.session.attach_path(file);
        }
    }

    /// A file's × in the strip.
    pub fn remove_waiting(&mut self, local: u64) {
        self.media.alts.remove(&local);
        self.session.discard_waiting(local);
    }

    /// The alt text field of the picture `local` in the strip, for its `on_edit`.
    pub fn alt_field(&mut self, local: u64) -> &mut TextEdit {
        self.media
            .alts
            .entry(local)
            .or_insert_with(|| TextEdit::new(""))
    }

    /// The alt text changed: the core keeps it with the draft.
    pub fn alt_changed(&mut self, local: u64) {
        let alt = self
            .media
            .alts
            .get(&local)
            .map(|field| field.text.clone())
            .unwrap_or_default();
        self.session.set_waiting_alt(local, &alt);
    }

    /// What the passing of time does for the composer: once the words have rested for
    /// [`PREVIEW_AFTER_MICROS`], their links are previewed. True when that changed what is
    /// on screen; the window is woken for it (`app::poll`).
    pub(crate) fn tick_media(&mut self, now_micros: i64) -> bool {
        let text = self.draft.text.clone();
        if text != self.media.previewed && self.media.typed.as_ref().is_none_or(|(t, _)| *t != text)
        {
            self.media.typed = Some((text, now_micros));
        }
        let mut changed = false;
        if let Some((text, at)) = self.media.typed.clone()
            && now_micros - at >= PREVIEW_AFTER_MICROS
        {
            self.media.typed = None;
            changed = self.session.preview_links(&text);
            self.media.previewed = text;
        }
        let synced = self.media.sync(&self.session);
        changed || synced
    }

    /// When the window should look at the draft's links again, in microseconds from
    /// `now_micros`.
    pub(crate) fn preview_wake(&self, now_micros: i64) -> Option<i64> {
        self.media
            .typed
            .as_ref()
            .map(|(_, at)| (at + PREVIEW_AFTER_MICROS - now_micros).max(0))
    }

    /// After a send or a change of conversation: the draft's links are looked at afresh.
    pub(crate) fn forget_typed(&mut self) {
        self.media.typed = None;
        self.media.previewed.clear();
    }

    // ---- Messages --------------------------------------------------------------------

    /// Tells the session which of pictures and cards Preferences draws.
    pub(crate) fn follow_media_settings(&mut self) {
        self.session.show_media(
            self.settings.images == Media::Shown,
            self.settings.embeds == Media::Shown,
        );
    }

    /// A picture of a message pressed: the viewer, with its display picture asked for.
    pub fn open_viewer(&mut self, attachment: &AttachmentSummary) {
        self.session.want_blob(&attachment.main);
        self.media.viewer = Some(Viewer {
            attachment: attachment.clone(),
            zoom: ZOOM_MIN,
            pan: [0.0, 0.0],
            grab: [0.0, 0.0],
        });
        self.media.seen = None;
    }

    /// Escape, the ×, or a press beside the picture.
    pub fn close_viewer(&mut self) {
        self.media.viewer = None;
    }

    /// Zooms the viewer in (`steps` above 0) or out; back to fitting the window at 0.
    pub fn zoom_viewer(&mut self, steps: i32) {
        let Some(viewer) = &mut self.media.viewer else {
            return;
        };
        viewer.zoom = if steps == 0 {
            ZOOM_MIN
        } else {
            (viewer.zoom * ZOOM_STEP.powi(steps)).clamp(ZOOM_MIN, ZOOM_MAX)
        };
        if viewer.zoom <= ZOOM_MIN {
            viewer.pan = [0.0, 0.0];
        }
    }

    /// Keys on the viewer: + and − zoom, 0 fits the window again. Escape is its modal's.
    pub fn viewer_key(&mut self, key: &KeyEvent) -> bool {
        match &key.key {
            Key::Text(t) if t == "+" || t == "=" => self.zoom_viewer(1),
            Key::Text(t) if t == "-" => self.zoom_viewer(-1),
            Key::Text(t) if t == "0" => self.zoom_viewer(0),
            _ => return false,
        }
        true
    }

    /// A drag on the viewer's picture moves it, while it is zoomed past the window.
    pub fn pan_viewer(&mut self, drag: &Drag) {
        let Some(viewer) = &mut self.media.viewer else {
            return;
        };
        if viewer.zoom <= ZOOM_MIN {
            return;
        }
        match drag.phase {
            rune::Phase::Down => viewer.grab = viewer.pan,
            _ if drag.cancelled() => viewer.pan = viewer.grab,
            _ => viewer.pan = [viewer.grab[0] + drag.dx, viewer.grab[1] + drag.dy],
        }
    }

    /// Save: the system's save dialog under the attachment's name, then the file.
    pub fn save_attachment(&mut self, attachment: &AttachmentSummary, tx: &mut Tx) {
        let attachment = attachment.clone();
        let options = FileDialogOptions {
            title: "Save".into(),
            file_name: Some(attachment.name.clone()),
            ..FileDialogOptions::default()
        };
        tx.save_file(options, move |s: &mut State, answer, _| match answer {
            Ok(Some(path)) => s.session.save_attachment(&attachment, &path),
            Ok(None) => {}
            Err(error) => {
                s.session.last_error = Some(format!("The file dialog could not open: {error}"));
            }
        });
    }

    /// A still GIF pressed under Reduce Motion: it plays.
    pub fn play_gif(&mut self, attachment_id: &str) {
        self.media.playing.insert(attachment_id.to_string());
        self.session.revision += 1;
    }

    /// A card pressed: the link as the message's words have it, in the browser.
    pub fn open_card(&mut self, url: &str, tx: &mut Tx) {
        if url.starts_with("https://") || url.starts_with("http://") {
            let _ = tx.open_url(url);
        }
    }

    /// While editing: an attachment's ×.
    pub fn remove_while_editing(&mut self, attachment_id: &str) {
        if !self.media.edit_removed.iter().any(|id| id == attachment_id) {
            self.media.edit_removed.push(attachment_id.to_string());
            self.session.revision += 1;
        }
    }

    /// While editing: a card's ×.
    pub fn drop_card_while_editing(&mut self, url: &str) {
        if !self.media.edit_dropped.iter().any(|u| u == url) {
            self.media.edit_dropped.push(url.to_string());
            self.session.revision += 1;
        }
    }

    pub(crate) fn react_media(&mut self, update: &MediaUpdate) {
        match update {
            MediaUpdate::Opened(Ok(path)) => self.media.to_open.push(path.clone()),
            MediaUpdate::Written { .. } => self.react_playback(update),
            MediaUpdate::Attached { .. } => {
                // A picture uploaded takes alt text: its field starts empty.
                let ready: Vec<u64> = self
                    .session
                    .media
                    .waiting
                    .iter()
                    .filter(|w| matches!(w.stage, Stage::Ready(_)) && w.is_picture())
                    .map(|w| w.local)
                    .collect();
                for local in ready {
                    self.media
                        .alts
                        .entry(local)
                        .or_insert_with(|| TextEdit::new(""));
                }
            }
            _ => {}
        }
    }
}

/// What a test or a scene hands the state as if a picture had arrived as it was sent: a
/// GIF, a poster, a card's picture, an image's display picture for the viewer.
pub fn fetched(key: &str, bytes: &[u8]) -> Update {
    Update::Media(MediaUpdate::Fetched {
        key: key.to_string(),
        picture: Ok(App::attachment_drawable(bytes).then(|| Picture::Bytes(Arc::from(bytes)))),
    })
}

/// The same for an image's display picture as a message draws it: made small, as the
/// session's fetch makes it.
pub fn fetched_small(blob: &BlobSummary, bytes: &[u8]) -> Update {
    Update::Media(MediaUpdate::Fetched {
        key: small_key(blob),
        picture: Ok(small(bytes)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_keeps_its_shape_and_is_never_drawn_larger() {
        // A wide photo fits the column; a tall one stops at 400.
        assert_eq!(fitted(4000, 3000, 520.0, 400.0), [520.0, 390.0]);
        assert_eq!(fitted(1000, 3000, 520.0, 400.0), [133.0, 400.0]);
        // A small one stays as it is.
        assert_eq!(fitted(120, 80, 520.0, 400.0), [120.0, 80.0]);
        // An unknown size is a square of the room there is.
        assert_eq!(fitted(0, 0, 520.0, 400.0), [400.0, 400.0]);
        // A sliver is still a point wide.
        assert_eq!(fitted(1, 100_000, 520.0, 400.0), [1.0, 400.0]);
    }

    #[test]
    fn several_pictures_stand_in_equal_squares() {
        assert_eq!(grid(2, 520.0, 200.0, 4.0), (2, 200.0));
        assert_eq!(grid(4, 520.0, 200.0, 4.0), (2, 200.0));
        assert_eq!(grid(5, 520.0, 200.0, 4.0), (3, 170.0));
        // A narrow column makes them smaller rather than wider than it.
        assert_eq!(grid(3, 300.0, 200.0, 4.0), (2, 148.0));
    }
}
