//! Pictures: the avatars of the people this account can see, and its own.
//!
//! An account names its picture by a path on the instance (protocol: `/api/v1/avatars/` and
//! an address), behind which are bytes the instance encoded itself and never changes. The
//! session fetches each once, a few at a time, keeps the bytes under the instance's
//! directory (`avatars/`, by address) so a later launch has them before the instance
//! answers, and holds them by URL for the views. A picture a signed profile names on
//! another host is not fetched: asking would tell that host who is looking.
//!
//! The bytes are held as bytes. Decoding them is the toolkit's, in `app`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumia_client::app::{SnapshotSummary, is_own_avatar};

use super::{Failure, Session, Update};

/// How many pictures are asked for at once.
const AT_ONCE: usize = 4;

/// The most a chosen file may be: the instance's own limit, checked before sending.
const MAX_UPLOAD_BYTES: u64 = 8 * 1024 * 1024;

/// What an avatar call reports.
#[derive(Debug, Clone)]
pub enum AvatarUpdate {
    /// A picture's bytes arrived, or the instance has none at that address.
    Fetched {
        url: String,
        bytes: Result<Arc<[u8]>, Failure>,
    },
    /// This account's picture was set or removed: its snapshot as it then is.
    Saved(Result<SnapshotSummary, Failure>),
}

/// What is known of the picture at a URL.
#[derive(Debug, Clone)]
enum Picture {
    /// Asked for, or waiting its turn.
    Wanted,
    Held(Arc<[u8]>),
    /// The instance has none there, or would not say: not asked again this launch.
    Missing,
}

/// The pictures held, by avatar URL.
#[derive(Debug, Default)]
pub struct Avatars {
    known: HashMap<String, Picture>,
    queue: VecDeque<String>,
    in_flight: usize,
    /// Bumped whenever a picture is gained or dropped, for whoever decodes them.
    pub revision: u64,
}

impl Avatars {
    /// Every picture held: its URL and its bytes.
    pub fn held(&self) -> impl Iterator<Item = (&str, &Arc<[u8]>)> {
        self.known
            .iter()
            .filter_map(|(url, picture)| match picture {
                Picture::Held(bytes) => Some((url.as_str(), bytes)),
                _ => None,
            })
    }
}

/// The file a picture is kept in: its address, which is all of the URL that varies.
fn kept_at(dir: &Path, url: &str) -> Option<PathBuf> {
    is_own_avatar(url).then(|| dir.join(url.rsplit('/').next().unwrap_or_default()))
}

/// What a file is taken to be, by its name. The instance tells the format from the bytes;
/// this only labels the request.
fn media_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

fn refused(reason: &str, message: &str) -> Failure {
    Failure {
        code: "INVALID_ARGUMENT".into(),
        message: message.into(),
        details: HashMap::from([("reason".to_string(), reason.to_string())]),
    }
}

impl Session {
    fn avatars_dir(&self) -> PathBuf {
        self.data_dir.join(&self.host).join("avatars")
    }

    /// The picture an account names, or nothing: its own from the snapshot, anyone else's
    /// from what the servers or a looked-up profile say of them.
    pub fn avatar_url_of(&self, account_id: &str) -> &str {
        if account_id.is_empty() {
            return "";
        }
        match (&self.snapshot, self.person(account_id)) {
            (Some(snapshot), _) if snapshot.account_id == account_id => &snapshot.avatar_url,
            (_, Some(person)) => &person.avatar_url,
            _ => "",
        }
    }

    /// The URL of an account's picture, when its bytes are held.
    pub fn picture_of(&self, account_id: &str) -> Option<&str> {
        let url = self.avatar_url_of(account_id);
        matches!(self.avatars.known.get(url), Some(Picture::Held(_))).then_some(url)
    }

    /// Looks over everyone known for pictures not yet held: each comes from the directory
    /// they are kept in when it is there, and is asked of the instance otherwise.
    pub(crate) fn want_pictures(&mut self) {
        let own = self.snapshot.as_ref().map(|s| s.avatar_url.as_str());
        let named = self.people.values().map(|p| p.avatar_url.as_str());
        let looked_up = self
            .profiles
            .values()
            .flatten()
            .map(|p| p.avatar_url.as_str());
        let new: Vec<String> = own
            .into_iter()
            .chain(named)
            .chain(looked_up)
            .filter(|url| is_own_avatar(url) && !self.avatars.known.contains_key(*url))
            .map(str::to_string)
            .collect();
        let dir = self.avatars_dir();
        for url in new {
            if self.avatars.known.contains_key(&url) {
                continue;
            }
            let kept = kept_at(&dir, &url).and_then(|file| std::fs::read(file).ok());
            match kept {
                Some(bytes) if !bytes.is_empty() => {
                    self.avatars.known.insert(url, Picture::Held(bytes.into()));
                    self.avatars.revision += 1;
                }
                _ => {
                    self.avatars.known.insert(url.clone(), Picture::Wanted);
                    self.avatars.queue.push_back(url);
                }
            }
        }
        self.fetch_pictures();
    }

    fn fetch_pictures(&mut self) {
        let Some(app) = self.client() else { return };
        while self.avatars.in_flight < AT_ONCE {
            let Some(url) = self.avatars.queue.pop_front() else {
                return;
            };
            self.avatars.in_flight += 1;
            let app = Arc::clone(&app);
            self.task(async move {
                let bytes = app.avatar(&url).await.map(Arc::from).map_err(Failure::from);
                Update::Avatar(AvatarUpdate::Fetched { url, bytes })
            });
        }
    }

    /// Makes the file at `path` this account's picture: read here, sent as it is, and
    /// turned into the instance's own square there. The answer is [`AvatarUpdate::Saved`].
    pub fn set_avatar(&self, path: &Path) {
        let Some(app) = self.client() else { return };
        let path = path.to_path_buf();
        self.task(async move {
            let saved = match std::fs::metadata(&path) {
                Ok(file) if file.len() > MAX_UPLOAD_BYTES => {
                    Err(refused("too_large", "the picture is over 8 MB"))
                }
                Ok(_) => match std::fs::read(&path) {
                    Ok(bytes) => app
                        .set_avatar(bytes, media_type(&path))
                        .await
                        .map_err(Failure::from),
                    Err(error) => Err(refused("unreadable", &error.to_string())),
                },
                Err(error) => Err(refused("unreadable", &error.to_string())),
            };
            Update::Avatar(AvatarUpdate::Saved(saved))
        });
    }

    /// Takes this account's picture away; the answer is [`AvatarUpdate::Saved`].
    pub fn remove_avatar(&self) {
        let Some(app) = self.client() else { return };
        self.task(async move {
            Update::Avatar(AvatarUpdate::Saved(
                app.remove_avatar().await.map_err(Failure::from),
            ))
        });
    }

    pub(super) fn apply_avatar(&mut self, update: &AvatarUpdate) {
        match update {
            AvatarUpdate::Fetched { url, bytes } => {
                self.avatars.in_flight = self.avatars.in_flight.saturating_sub(1);
                match bytes {
                    Ok(bytes) => {
                        // Kept for the next launch; a failure to keep costs one more fetch.
                        let dir = self.avatars_dir();
                        if let Some(file) = kept_at(&dir, url)
                            && std::fs::create_dir_all(&dir).is_ok()
                        {
                            let _ = std::fs::write(file, bytes);
                        }
                        self.avatars
                            .known
                            .insert(url.clone(), Picture::Held(Arc::clone(bytes)));
                        self.avatars.revision += 1;
                    }
                    Err(_) => {
                        self.avatars.known.insert(url.clone(), Picture::Missing);
                    }
                }
                self.fetch_pictures();
            }
            AvatarUpdate::Saved(Ok(snapshot)) => {
                // The lists that show this person were read before the change.
                let me = snapshot.account_id.as_str();
                let listed = self.members.values_mut().flatten();
                for person in listed.chain(self.people.values_mut()) {
                    if person.account_id == me {
                        person.avatar_url.clone_from(&snapshot.avatar_url);
                    }
                }
                self.snapshot = Some(snapshot.clone());
                self.want_pictures();
            }
            AvatarUpdate::Saved(Err(_)) => {}
        }
    }

    /// Forgets every picture held in memory; the files kept stay.
    pub(super) fn forget_pictures(&mut self) {
        let revision = self.avatars.revision + 1;
        self.avatars = Avatars {
            revision,
            ..Avatars::default()
        };
    }
}
