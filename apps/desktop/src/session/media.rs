//! Attachments and link cards (specs/media.md): what the composer holds waiting to be sent,
//! the cards it previews while the person writes, and the pictures of the messages on
//! screen as bytes.
//!
//! **Attaching.** A file chosen, pasted or dropped goes through the core at once
//! (`App::attach`: processed on this computer, sealed and uploaded into the conversation),
//! and waits in the composer's strip ([`Waiting`]) until a send names it. The core does the
//! work; the shell hands it the bytes and the name only. It has no decoder of its own for
//! HEIC or HEIF and makes no poster frame of a video, so a HEIC picture goes as a file and a
//! video without a poster (spec 3.1, 3.3).
//!
//! **Previews.** Once the person stops typing for a moment (the window decides when,
//! `app::media`), the links the core says may have cards (`App::card_links`, at most three, and none in a server channel of an instance that makes no cards)
//! are each asked of `App::card_preview`: the instance builds a server channel's, this
//! computer a personal conversation's. A × puts one away, and a send asks for the cards of
//! the links whose preview was not put away (spec 6.4, 6.5).
//!
//! **Pictures.** The thumbnails (an image's, a GIF's first frame, a video's poster), a GIF
//! itself and the cards' pictures of the conversation on screen are fetched a few at a
//! time when its messages are read, opened by the core (which checks them against their
//! hash and keeps them, under `<host>/media/`), and held here as bytes. A picture is
//! someone else's choice of bytes: one the core says not to decode (`App::attachment_drawable`)
//! is held as such and drawn as a file. The display picture is fetched only for the viewer.
//!
//! Decoding is the toolkit's, in `app`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumia_client::app::{
    App, AttachmentDraft, AttachmentInput, AttachmentSummary, BlobSummary, CardSummary,
    UploadLimitsSummary,
};

use super::{Conversation, Entry, Failure, Session, Update};

/// How many pictures are asked for at once.
const AT_ONCE: usize = 4;

/// The largest picture read to be attached: the core makes it smaller, but it is read whole
/// first. A larger file of any other kind is refused by the file cap before it is read.
const PICTURE_READ_MAX: u64 = 64 * 1024 * 1024;

/// What a media call reports.
#[derive(Debug, Clone)]
pub enum MediaUpdate {
    /// What the instance takes of attachments: `None` when it keeps none.
    /// What the instance takes of attachments, and whether it makes server link cards.
    Limits(Result<(Option<UploadLimitsSummary>, bool), Failure>),
    /// A file waiting in the composer was uploaded, or could not be.
    Attached {
        local: u64,
        result: Result<AttachmentDraft, Failure>,
    },
    /// A picture's bytes, opened, by its key ([`blob_key`], or a card picture's path).
    Fetched {
        key: String,
        bytes: Result<Arc<[u8]>, Failure>,
    },
    /// A link's card for the composer: `None` when the page gives none.
    Preview {
        channel_id: String,
        url: String,
        card: Result<Option<CardSummary>, Failure>,
    },
    /// An attachment was written where the person chose.
    Saved(Result<PathBuf, Failure>),
    /// A video or a recording was written where the system's player can open it.
    Opened(Result<PathBuf, Failure>),
    /// A draft taken out of the composer was taken off the instance (best effort).
    Discarded,
}

/// Where a file waiting in the composer is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Being read, processed and uploaded.
    Uploading,
    /// Uploaded: a send can name it.
    Ready(AttachmentDraft),
    /// Why it cannot be sent, in words.
    Failed(String),
}

/// A file in the composer's strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// This window's own number for it, before the core has named it.
    pub local: u64,
    pub channel_id: String,
    pub name: String,
    /// The file as chosen, until the core says what it will be sent at.
    pub size_bytes: u64,
    pub stage: Stage,
}

impl Waiting {
    /// The core's draft, once uploaded.
    pub fn draft(&self) -> Option<&AttachmentDraft> {
        match &self.stage {
            Stage::Ready(draft) => Some(draft),
            _ => None,
        }
    }

    /// Whether it goes as a picture, which takes alt text: what the core made of it.
    pub fn is_picture(&self) -> bool {
        self.draft()
            .is_some_and(|d| d.kind == "image" || d.kind == "gif")
    }

    /// Its size as it will be sent.
    pub fn size(&self) -> u64 {
        self.draft().map_or(self.size_bytes, |d| d.size_bytes)
    }
}

/// A link's card while the person writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub channel_id: String,
    pub url: String,
    /// The card, once asked; `None` while asked and when the page gave none.
    pub card: Option<CardSummary>,
    pub loading: bool,
    /// Put away with its ×: the send asks for no card for this link.
    pub dismissed: bool,
}

/// What is known of a picture.
#[derive(Debug, Clone)]
enum Held {
    Wanted,
    Bytes(Arc<[u8]>),
    /// Its header names more than may be decoded: drawn as a file.
    Undrawable,
    /// Not served, or would not open: not asked again while the conversation is open.
    Missing,
}

/// Where a picture comes from.
#[derive(Debug, Clone)]
enum Source {
    Blob(BlobSummary),
    Card(String),
}

/// Attachments and cards as the session holds them.
#[derive(Debug)]
pub struct MediaState {
    /// What the instance takes; `None` until it answers, and when it keeps no attachments,
    /// in which case the composer offers no way to attach.
    pub limits: Option<UploadLimitsSummary>,
    /// Whether the instance makes link cards in server channels; a composer there
    /// previews none until it says so.
    pub link_cards: bool,
    asked_limits: bool,
    /// The files in the composers' strips, every conversation's, in the order added.
    pub waiting: Vec<Waiting>,
    next_local: u64,
    /// Removed while still uploading: discarded as soon as the upload answers.
    discarded: HashSet<u64>,
    /// The cards previewed while writing, every conversation's.
    pub previews: Vec<Preview>,
    held: HashMap<String, Held>,
    queue: VecDeque<(String, Source)>,
    in_flight: usize,
    /// Whether Preferences draws pictures and cards: what is not drawn is not fetched.
    show_images: bool,
    show_embeds: bool,
    /// Bumped whenever a picture is gained or dropped, or a draft changes, for whoever
    /// decodes them.
    pub revision: u64,
}

impl Default for MediaState {
    fn default() -> Self {
        Self {
            limits: None,
            link_cards: false,
            asked_limits: false,
            waiting: Vec::new(),
            next_local: 0,
            discarded: HashSet::new(),
            previews: Vec::new(),
            held: HashMap::new(),
            queue: VecDeque::new(),
            in_flight: 0,
            show_images: true,
            show_embeds: true,
            revision: 0,
        }
    }
}

impl MediaState {
    /// Every picture held: its key and its bytes.
    pub fn held(&self) -> impl Iterator<Item = (&str, &Arc<[u8]>)> {
        self.held.iter().filter_map(|(key, held)| match held {
            Held::Bytes(bytes) => Some((key.as_str(), bytes)),
            _ => None,
        })
    }

    /// The bytes of the picture at `key`, when held.
    pub fn bytes(&self, key: &str) -> Option<&Arc<[u8]>> {
        match self.held.get(key) {
            Some(Held::Bytes(bytes)) => Some(bytes),
            _ => None,
        }
    }

    /// Whether the picture at `key` arrived and may not be decoded: drawn as a file.
    pub fn undrawable(&self, key: &str) -> bool {
        matches!(self.held.get(key), Some(Held::Undrawable))
    }
}

/// The key a blob's picture is held under.
pub fn blob_key(blob: &BlobSummary) -> String {
    blob.blob_id.clone()
}

/// The key a card's picture is held under: its path on the instance, or its blob's.
pub fn card_key(card: &CardSummary) -> Option<String> {
    if !card.image_url.is_empty() {
        Some(card.image_url.clone())
    } else {
        card.image.as_ref().map(blob_key)
    }
}

/// Whether an attachment is drawn as a picture: an image or a GIF, and a video by its poster.
/// One the server let go is a file row (specs/media.md 4.5).
pub fn is_picture(attachment: &AttachmentSummary) -> bool {
    !attachment.expired && matches!(attachment.kind.as_str(), "image" | "gif")
}

/// The extension a video or a recording is written under for the system's player: from its
/// media type, never from the name its sender gave it, so a file that says it is a video
/// cannot be opened as a program. `None` for anything else.
pub fn player_extension(media_type: &str) -> Option<&'static str> {
    let essence = media_type.split(';').next().unwrap_or("").trim();
    Some(match essence.to_ascii_lowercase().as_str() {
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        "audio/mpeg" => "mp3",
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/aac" => "aac",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/flac" | "audio/x-flac" => "flac",
        _ => return None,
    })
}

/// Whether a file's name says it is a picture, so it may be larger than the file cap before
/// the core makes it smaller. The core tells the format from the bytes; this only decides
/// whether to read it.
fn named_picture(name: &str) -> bool {
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    matches!(
        extension.as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "heic" | "heif")
    )
}

/// A size as a file row says it: "820 KB", "2.4 MB", "5 GB". In 1,000s, as the systems' own
/// file sizes and every Lumia client say them: one decimal under 10, whole above.
pub fn size_words(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return if bytes == 1 {
            "1 byte".into()
        } else {
            format!("{bytes} bytes")
        };
    }
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 999.5 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let shown = if value < 9.95 {
        let one = format!("{value:.1}");
        one.strip_suffix(".0").map_or(one.clone(), str::to_string)
    } else {
        format!("{value:.0}")
    };
    format!("{shown} {}", UNITS[unit])
}

/// Why an attachment could not be sent, as its row in the strip says it.
fn refusal_words(failure: &Failure) -> String {
    match failure.code.as_str() {
        "PAYLOAD_TOO_LARGE" => "Too large for this instance.".into(),
        "RESOURCE_EXHAUSTED"
            if failure.details.get("state").map(String::as_str) == Some("storage") =>
        {
            "This server is out of space for files.".into()
        }
        "PERMISSION_DENIED" => "You can’t attach files here.".into(),
        _ => {
            let mut words = failure.message.trim().to_string();
            if let Some(first) = words.get(..1) {
                let upper = first.to_uppercase();
                words.replace_range(..1, &upper);
            }
            if !words.ends_with('.') {
                words.push('.');
            }
            words
        }
    }
}

impl Session {
    pub(super) fn media_dir(&self) -> PathBuf {
        self.data_dir.join(&self.host).join("media")
    }

    /// Asks once what the instance takes of attachments ([`MediaUpdate::Limits`]).
    pub(super) fn ask_upload_limits(&mut self) {
        let Some(app) = self.client() else { return };
        if std::mem::replace(&mut self.media.asked_limits, true) {
            return;
        }
        self.task(async move {
            let limits = app.upload_limits().await.map_err(Failure::from);
            let cards = app.server_link_cards();
            Update::Media(MediaUpdate::Limits(limits.map(|l| (l, cards))))
        });
    }

    /// Whether the composer offers a way to attach: the instance keeps attachments.
    pub fn can_attach(&self) -> bool {
        self.media.limits.is_some()
    }

    /// The files waiting in the open conversation's composer.
    pub fn waiting_here(&self) -> impl Iterator<Item = &Waiting> {
        let here = self.conversation.as_ref().map(Conversation::id);
        self.media
            .waiting
            .iter()
            .filter(move |w| Some(w.channel_id.as_str()) == here)
    }

    /// Whether a file of the open conversation's composer is still being uploaded, which
    /// holds the send back.
    pub fn uploading_here(&self) -> bool {
        self.waiting_here().any(|w| w.stage == Stage::Uploading)
    }

    /// Attaches the file at `path` to the open conversation's composer: read, processed and
    /// uploaded off this thread, waiting in the strip meanwhile.
    pub fn attach_path(&mut self, path: &Path) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let path = path.to_path_buf();
        self.attach_with(name, size, move || std::fs::read(&path));
    }

    /// Attaches bytes the person pasted, under `name`.
    pub fn attach_bytes(&mut self, name: &str, bytes: Vec<u8>) {
        let size = bytes.len() as u64;
        self.attach_with(name.to_string(), size, move || Ok(bytes));
    }

    fn attach_with(
        &mut self,
        name: String,
        size: u64,
        read: impl FnOnce() -> std::io::Result<Vec<u8>> + Send + 'static,
    ) {
        let (Some(app), Some(conversation), Some(limits)) =
            (self.client(), self.conversation.clone(), self.media.limits)
        else {
            return;
        };
        let channel_id = conversation.id().to_string();
        self.media.next_local += 1;
        let local = self.media.next_local;
        let picture = named_picture(&name);
        // Refused before the file is read: anything over the file cap that the core would
        // not make smaller, and a picture too large to read at all.
        let refused = if picture && size > PICTURE_READ_MAX {
            Some(format!(
                "Too large to attach: pictures can be up to {}.",
                size_words(PICTURE_READ_MAX)
            ))
        } else if !picture && size + 16 > limits.file_max_bytes {
            Some(format!(
                "Too large to attach: files can be up to {} here.",
                size_words(limits.file_max_bytes.saturating_sub(16))
            ))
        } else {
            None
        };
        self.media.waiting.push(Waiting {
            local,
            channel_id: channel_id.clone(),
            name: name.clone(),
            size_bytes: size,
            stage: refused.clone().map_or(Stage::Uploading, Stage::Failed),
        });
        self.media.revision += 1;
        self.revision += 1;
        if refused.is_some() {
            return;
        }
        self.task_local(move || async move {
            let result = match read() {
                Ok(bytes) => app
                    .attach(
                        &channel_id,
                        AttachmentInput {
                            bytes,
                            name,
                            ..AttachmentInput::default()
                        },
                    )
                    .await
                    .map_err(Failure::from),
                Err(error) => Err(Failure {
                    message: format!("could not read it: {error}"),
                    ..Failure::default()
                }),
            };
            Update::Media(MediaUpdate::Attached { local, result })
        });
    }

    /// Takes a file out of the composer's strip, and its upload off the instance.
    pub fn discard_waiting(&mut self, local: u64) {
        let Some(at) = self.media.waiting.iter().position(|w| w.local == local) else {
            return;
        };
        let waiting = self.media.waiting.remove(at);
        self.media.revision += 1;
        self.revision += 1;
        match waiting.stage {
            Stage::Uploading => {
                self.media.discarded.insert(local);
            }
            Stage::Ready(draft) => self.discard_draft(draft.draft_id),
            Stage::Failed(_) => {}
        }
    }

    fn discard_draft(&self, draft_id: String) {
        let Some(app) = self.client() else { return };
        self.task(async move {
            app.discard_attachment(&draft_id).await;
            Update::Media(MediaUpdate::Discarded)
        });
    }

    /// Sets a waiting picture's alt text, which the core keeps with its draft.
    pub fn set_waiting_alt(&mut self, local: u64, alt: &str) {
        let Some(app) = self.client() else { return };
        let Some(waiting) = self.media.waiting.iter_mut().find(|w| w.local == local) else {
            return;
        };
        if let Stage::Ready(draft) = &mut waiting.stage
            && let Ok(updated) = app.set_attachment_alt(&draft.draft_id, alt)
        {
            *draft = updated;
        }
    }

    /// The drafts the next send of the open conversation carries: those uploaded.
    pub(super) fn drafts_here(&self) -> Vec<AttachmentDraft> {
        self.waiting_here()
            .filter_map(Waiting::draft)
            .cloned()
            .collect()
    }

    /// After a send: what it carried leaves the strip, and the previews go.
    pub(super) fn sent_media(&mut self, channel_id: &str, drafts: &[AttachmentDraft]) {
        self.media.waiting.retain(|w| {
            w.channel_id != channel_id
                || !w
                    .draft()
                    .is_some_and(|d| drafts.iter().any(|s| s.draft_id == d.draft_id))
        });
        self.media.previews.retain(|p| p.channel_id != channel_id);
        self.media.revision += 1;
    }

    // ---- Previews -------------------------------------------------------------------

    /// The cards previewed in the open conversation's composer, those not put away.
    pub fn previews_here(&self) -> impl Iterator<Item = &Preview> {
        let here = self.conversation.as_ref().map(Conversation::id);
        self.media
            .previews
            .iter()
            .filter(move |p| Some(p.channel_id.as_str()) == here && !p.dismissed)
    }

    /// Follows the draft's links: a preview asked for each new one, those gone dropped.
    /// True when what is previewed changed.
    pub fn preview_links(&mut self, body: &str) -> bool {
        let Some(channel_id) = self.conversation.as_ref().map(|c| c.id().to_string()) else {
            return false;
        };
        // None where the conversation cannot have cards (a server channel of an instance
        // that makes none), so nothing is shown waiting for one. A personal one's are this
        // computer's own.
        let personal = matches!(self.conversation, Some(Conversation::Dm(_)));
        let links = if personal || self.media.link_cards {
            App::card_links(body)
        } else {
            Vec::new()
        };
        let before = self.media.previews.len();
        self.media
            .previews
            .retain(|p| p.channel_id != channel_id || links.contains(&p.url));
        let mut changed = self.media.previews.len() != before;
        let Some(app) = self.client() else {
            return changed;
        };
        for url in links {
            let known = self
                .media
                .previews
                .iter()
                .any(|p| p.channel_id == channel_id && p.url == url);
            if known {
                continue;
            }
            changed = true;
            self.media.previews.push(Preview {
                channel_id: channel_id.clone(),
                url: url.clone(),
                card: None,
                loading: true,
                dismissed: false,
            });
            let (app, channel_id) = (Arc::clone(&app), channel_id.clone());
            self.task_local(move || async move {
                let card = app
                    .card_preview(&channel_id, &url)
                    .await
                    .map_err(Failure::from);
                Update::Media(MediaUpdate::Preview {
                    channel_id,
                    url,
                    card,
                })
            });
        }
        if changed {
            self.media.revision += 1;
        }
        changed
    }

    /// A preview's ×: the send asks for no card for that link.
    pub fn dismiss_preview(&mut self, url: &str) {
        let here = self.conversation.as_ref().map(|c| c.id().to_string());
        for preview in &mut self.media.previews {
            if Some(&preview.channel_id) == here.as_ref() && preview.url == url {
                preview.dismissed = true;
            }
        }
        self.media.revision += 1;
    }

    /// The links of `body` whose cards a send asks for: those whose preview was not put
    /// away, previewed yet or not (the instance builds what it has not, after the send).
    pub(super) fn kept_links(&self, channel_id: &str, body: &str) -> Vec<String> {
        App::card_links(body)
            .into_iter()
            .filter(|url| {
                !self
                    .media
                    .previews
                    .iter()
                    .any(|p| p.channel_id == channel_id && p.url == *url && p.dismissed)
            })
            .collect()
    }

    // ---- Pictures -------------------------------------------------------------------

    /// Says which of pictures and cards Preferences draws; what is not drawn is not fetched.
    pub fn show_media(&mut self, images: bool, embeds: bool) {
        if (self.media.show_images, self.media.show_embeds) != (images, embeds) {
            self.media.show_images = images;
            self.media.show_embeds = embeds;
            self.want_media();
        }
    }

    /// Looks over the messages on screen for pictures not yet held: the thumbnails of
    /// pictures and posters, a GIF itself, and the cards' pictures.
    pub fn want_media(&mut self) {
        let mut wanted: Vec<(String, Source)> = Vec::new();
        for entry in self.entries.iter().filter(|e| !e.deleted) {
            if self.media.show_images {
                for attachment in entry.attachments.iter().filter(|a| !a.expired) {
                    if let Some(thumbnail) = &attachment.thumbnail
                        && matches!(attachment.kind.as_str(), "image" | "gif" | "video")
                    {
                        wanted.push((blob_key(thumbnail), Source::Blob(thumbnail.clone())));
                    }
                    if attachment.kind == "gif" {
                        wanted.push((
                            blob_key(&attachment.main),
                            Source::Blob(attachment.main.clone()),
                        ));
                    }
                }
            }
            if self.media.show_embeds {
                for card in &entry.cards {
                    if !card.image_url.is_empty() {
                        wanted.push((card.image_url.clone(), Source::Card(card.image_url.clone())));
                    } else if let Some(image) = &card.image {
                        wanted.push((blob_key(image), Source::Blob(image.clone())));
                    }
                }
            }
        }
        for (key, source) in wanted {
            self.want(key, source);
        }
        self.fetch_media();
    }

    /// Asks for one blob's picture: an image's display picture, for the viewer.
    pub fn want_blob(&mut self, blob: &BlobSummary) {
        self.want(blob_key(blob), Source::Blob(blob.clone()));
        self.fetch_media();
    }

    fn want(&mut self, key: String, source: Source) {
        if self.media.held.contains_key(&key) {
            return;
        }
        self.media.held.insert(key.clone(), Held::Wanted);
        self.media.queue.push_back((key, source));
    }

    fn fetch_media(&mut self) {
        let Some(app) = self.client() else { return };
        while self.media.in_flight < AT_ONCE {
            let Some((key, source)) = self.media.queue.pop_front() else {
                return;
            };
            self.media.in_flight += 1;
            let app = Arc::clone(&app);
            self.task(async move {
                let bytes = match source {
                    Source::Blob(blob) => app.attachment_bytes(&blob).await,
                    Source::Card(url) => app.card_image(&url).await,
                };
                Update::Media(MediaUpdate::Fetched {
                    key,
                    bytes: bytes.map(Arc::from).map_err(Failure::from),
                })
            });
        }
    }

    /// Writes an attachment where the person chose: fetched and opened first, unless the
    /// core already holds it.
    pub fn save_attachment(&self, attachment: &AttachmentSummary, path: &Path) {
        let Some(app) = self.client() else { return };
        let (blob, path) = (attachment.main.clone(), path.to_path_buf());
        self.task(async move {
            let saved = match app.attachment_bytes(&blob).await {
                Ok(bytes) => std::fs::write(&path, bytes)
                    .map(|()| path)
                    .map_err(|e| Failure {
                        message: e.to_string(),
                        ..Failure::default()
                    }),
                Err(error) => Err(Failure::from(error)),
            };
            Update::Media(MediaUpdate::Saved(saved))
        });
    }

    /// Writes a video or a recording under this computer's temporary directory, named by
    /// its id and an extension its media type gives ([`player_extension`]), for the system's
    /// player to open. Nothing else is written there.
    pub fn open_attachment(&self, attachment: &AttachmentSummary) {
        let Some(app) = self.client() else { return };
        let Some(extension) = player_extension(&attachment.media_type) else {
            return;
        };
        let blob = attachment.main.clone();
        let file = std::env::temp_dir()
            .join("lumia-media")
            .join(format!("{}.{extension}", attachment.attachment_id));
        self.task(async move {
            let written = match app.attachment_bytes(&blob).await {
                Ok(bytes) => file
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&file, bytes))
                    .map(|()| file)
                    .map_err(|e| Failure {
                        message: e.to_string(),
                        ..Failure::default()
                    }),
                Err(error) => Err(Failure::from(error)),
            };
            Update::Media(MediaUpdate::Opened(written))
        });
    }

    /// Forgets the pictures held for another conversation's messages; what the core keeps
    /// on disk stays.
    pub(super) fn forget_media(&mut self) {
        let keep = MediaState {
            limits: self.media.limits,
            asked_limits: self.media.asked_limits,
            waiting: std::mem::take(&mut self.media.waiting),
            next_local: self.media.next_local,
            discarded: std::mem::take(&mut self.media.discarded),
            previews: std::mem::take(&mut self.media.previews),
            show_images: self.media.show_images,
            show_embeds: self.media.show_embeds,
            // What is on its way still arrives, and is counted off.
            in_flight: self.media.in_flight,
            revision: self.media.revision + 1,
            ..MediaState::default()
        };
        self.media = keep;
    }

    /// Forgets everything of the account's attachments, on the way out of it.
    pub(super) fn forget_all_media(&mut self) {
        let revision = self.media.revision + 1;
        self.media = MediaState {
            show_images: self.media.show_images,
            show_embeds: self.media.show_embeds,
            revision,
            ..MediaState::default()
        };
    }

    pub(super) fn apply_media(&mut self, update: &MediaUpdate) {
        match update {
            MediaUpdate::Limits(Ok((limits, cards))) => {
                self.media.limits = *limits;
                self.media.link_cards = *cards;
                self.revision += 1;
            }
            // Asked again with the next snapshot.
            MediaUpdate::Limits(Err(_)) => self.media.asked_limits = false,
            MediaUpdate::Attached { local, result } => {
                if self.media.discarded.remove(local) {
                    if let Ok(draft) = result {
                        self.discard_draft(draft.draft_id.clone());
                    }
                    return;
                }
                if let Some(waiting) = self.media.waiting.iter_mut().find(|w| w.local == *local) {
                    waiting.stage = match result {
                        Ok(draft) => Stage::Ready(draft.clone()),
                        Err(failure) => Stage::Failed(refusal_words(failure)),
                    };
                    self.media.revision += 1;
                    self.revision += 1;
                }
            }
            MediaUpdate::Fetched { key, bytes } => {
                self.media.in_flight = self.media.in_flight.saturating_sub(1);
                let held = match bytes {
                    Ok(bytes) if App::attachment_drawable(bytes) => Held::Bytes(Arc::clone(bytes)),
                    Ok(_) => Held::Undrawable,
                    Err(_) => Held::Missing,
                };
                self.media.held.insert(key.clone(), held);
                self.media.revision += 1;
                self.revision += 1;
                self.fetch_media();
            }
            MediaUpdate::Preview {
                channel_id,
                url,
                card,
            } => {
                if let Some(preview) = self
                    .media
                    .previews
                    .iter_mut()
                    .find(|p| p.channel_id == *channel_id && p.url == *url)
                {
                    preview.loading = false;
                    preview.card = card.clone().ok().flatten();
                    self.media.revision += 1;
                }
            }
            MediaUpdate::Saved(Err(failure)) => {
                self.last_error = Some(format!("Not saved: {}", failure.message));
            }
            MediaUpdate::Opened(Err(failure)) => {
                self.last_error = Some(format!("Could not open it: {}", failure.message));
            }
            MediaUpdate::Saved(Ok(_)) | MediaUpdate::Opened(Ok(_)) | MediaUpdate::Discarded => {}
        }
    }
}

/// The attachments of a message that are still there after an edit dropping `removed`.
pub fn kept_attachments<'a>(
    entry: &'a Entry,
    removed: &'a [String],
) -> impl Iterator<Item = &'a AttachmentSummary> {
    entry
        .attachments
        .iter()
        .filter(move |a| !removed.contains(&a.attachment_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_as_a_file_row_says_them() {
        assert_eq!(size_words(1), "1 byte");
        assert_eq!(size_words(512), "512 bytes");
        assert_eq!(size_words(999), "999 bytes");
        assert_eq!(size_words(1000), "1 KB");
        assert_eq!(size_words(1500), "1.5 KB");
        assert_eq!(size_words(820_000), "820 KB");
        assert_eq!(size_words(999_600), "1 MB");
        assert_eq!(size_words(2_400_000), "2.4 MB");
        assert_eq!(size_words(10 * 1024 * 1024), "10 MB");
        assert_eq!(size_words(5_000_000_000), "5 GB");
    }

    #[test]
    fn only_a_known_media_type_is_written_for_the_player() {
        assert_eq!(player_extension("video/mp4"), Some("mp4"));
        assert_eq!(player_extension("audio/ogg; codecs=opus"), Some("ogg"));
        assert_eq!(player_extension("VIDEO/QuickTime"), Some("mov"));
        assert_eq!(player_extension("application/x-msdownload"), None);
        assert_eq!(player_extension("text/html"), None);
        assert_eq!(player_extension(""), None);
    }

    #[test]
    fn a_picture_is_known_by_its_name_only_to_decide_whether_to_read_it() {
        assert!(named_picture("IMG_0001.HEIC"));
        assert!(named_picture("cat.jpeg"));
        assert!(!named_picture("notes.pdf"));
        assert!(!named_picture("jpeg"));
    }

    #[test]
    fn a_refusal_is_said_in_words() {
        let failure = |code: &str, message: &str| Failure {
            code: code.into(),
            message: message.into(),
            ..Failure::default()
        };
        assert_eq!(
            refusal_words(&failure("PAYLOAD_TOO_LARGE", "x")),
            "Too large for this instance."
        );
        let full = Failure {
            details: [("state".to_string(), "storage".to_string())].into(),
            ..failure("RESOURCE_EXHAUSTED", "quota exceeded")
        };
        assert_eq!(
            refusal_words(&full),
            "This server is out of space for files."
        );
        assert_eq!(
            refusal_words(&failure(
                "",
                "this picture is too large to send, even made smaller"
            )),
            "This picture is too large to send, even made smaller."
        );
    }
}
