//! Pictures: the avatars of the people this account can see, and its own.
//!
//! An account names its picture by a path on the instance (protocol: `/api/v1/avatars/` and
//! an address), behind which are bytes the instance encoded itself and never changes. The
//! session fetches each once, a few at a time, keeps the bytes under the instance's
//! directory (`avatars/`, by address) so a later launch has them before the instance
//! answers, and holds them by URL for the views. A picture a signed profile names on
//! another host is not fetched: asking would tell that host who is looking.
//!
//! A server's own emoji (specs/emoji.md 3, 10) are pictures of the same kind, made by the
//! instance and served by address, and are fetched and kept the same way. One the server no
//! longer lists (removed) is dropped, from memory and from what is kept (spec 7).
//!
//! A picture that could not be fetched (the network down a moment, the instance busy) is
//! asked for again after a while, longer each time ([`retry_after`]); one the instance says
//! it does not have is not. One whose bytes would not decode is let go, file and all, and
//! fetched again the same way ([`Session::picture_failed`]): a file half written when the
//! computer stopped is the likely cause, not the instance.
//!
//! The bytes are held as bytes. Decoding them is the toolkit's, in `app`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lumia_client::app::{SnapshotSummary, is_own_avatar};

use super::{Failure, Session, Update};

/// How many pictures are asked for at once.
const AT_ONCE: usize = 4;

/// The most a chosen file may be: the instance's own limit, checked before sending.
const MAX_UPLOAD_BYTES: u64 = 8 * 1024 * 1024;

/// How long after its `tries`-th failure a picture is asked for again: 2 s, doubling, at
/// most five minutes.
pub fn retry_after(tries: u32) -> Duration {
    Duration::from_secs((2u64 << tries.min(8)).min(300))
}

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
    /// Could not be had: asked for again at `retry`, or never (`None`) when the instance
    /// said it has none there.
    Missing {
        retry: Option<Instant>,
    },
}

/// The pictures held, by avatar URL.
#[derive(Debug, Default)]
pub struct Avatars {
    known: HashMap<String, Picture>,
    queue: VecDeque<String>,
    in_flight: usize,
    /// The emoji pictures wanted so far: one no server lists any more is dropped.
    emoji: HashSet<String>,
    /// Emoji pictures dropped: one arriving after is not kept.
    gone: HashSet<String>,
    /// How many times in a row each picture could not be had, for how long to wait.
    tries: HashMap<String, u32>,
    /// When the first picture waiting to be asked for again is due.
    next_retry: Option<Instant>,
    /// Bumped whenever a picture is gained or dropped, for whoever decodes them.
    pub revision: u64,
}

impl Avatars {
    /// The bytes held for a URL.
    pub fn bytes_of(&self, url: &str) -> Option<&Arc<[u8]>> {
        match self.known.get(url) {
            Some(Picture::Held(bytes)) => Some(bytes),
            _ => None,
        }
    }

    /// Whether the picture at a URL could not be had (it may be asked for again later).
    pub fn failed(&self, url: &str) -> bool {
        matches!(self.known.get(url), Some(Picture::Missing { .. }))
    }

    /// Whether a URL is one of the servers' emoji (as opposed to a person's or a server's
    /// picture).
    pub fn is_emoji(&self, url: &str) -> bool {
        self.emoji.contains(url)
    }

    /// When a picture that could not be had is next to be asked for, if one is.
    pub fn next_retry(&self) -> Option<Instant> {
        self.next_retry
    }

    /// Notes that the picture at `url` could not be had: asked for again after a while,
    /// unless `for_good`.
    fn missed(&mut self, url: &str, for_good: bool) {
        let retry = (!for_good).then(|| {
            let tries = self.tries.entry(url.to_string()).or_default();
            *tries += 1;
            Instant::now() + retry_after(*tries - 1)
        });
        if let Some(at) = retry {
            self.next_retry = Some(self.next_retry.map_or(at, |next| next.min(at)));
        }
        self.known
            .insert(url.to_string(), Picture::Missing { retry });
    }

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

/// The bytes of the picture file at `path` and what it is taken to be, or the refusal the
/// instance would have given: an account's photo and a server's picture are read alike.
pub(super) fn read_picture(path: &Path) -> Result<(Vec<u8>, &'static str), Failure> {
    match std::fs::metadata(path) {
        Ok(file) if file.len() > MAX_UPLOAD_BYTES => {
            Err(refused("too_large", "the picture is over 8 MB"))
        }
        Ok(_) => std::fs::read(path)
            .map(|bytes| (bytes, media_type(path)))
            .map_err(|error| refused("unreadable", &error.to_string())),
        Err(error) => Err(refused("unreadable", &error.to_string())),
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
    pub(super) fn avatars_dir(&self) -> PathBuf {
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
        let servers = self
            .snapshot
            .iter()
            .flat_map(|s| s.servers.iter().map(|server| server.icon_url.as_str()));
        let named = self.people.values().map(|p| p.avatar_url.as_str());
        let looked_up = self
            .profiles
            .values()
            .flatten()
            .map(|p| p.avatar_url.as_str());
        let emoji: HashSet<String> = self
            .snapshot
            .iter()
            .flat_map(|s| s.servers.iter().flat_map(|server| server.emoji.iter()))
            .map(|e| e.image_url.clone())
            .filter(|url| is_own_avatar(url))
            .collect();
        let dropped: Vec<String> = self.avatars.emoji.difference(&emoji).cloned().collect();
        let dir = self.avatars_dir();
        for url in dropped {
            if self.avatars.known.remove(&url).is_some() {
                self.avatars.revision += 1;
            }
            self.avatars.queue.retain(|queued| *queued != url);
            if let Some(file) = kept_at(&dir, &url) {
                let _ = std::fs::remove_file(file);
            }
            self.avatars.gone.insert(url);
        }
        let new: Vec<String> = own
            .into_iter()
            .chain(servers)
            .chain(named)
            .chain(looked_up)
            .chain(emoji.iter().map(String::as_str))
            .filter(|url| is_own_avatar(url) && !self.avatars.known.contains_key(*url))
            .map(str::to_string)
            .collect();
        self.avatars.emoji = emoji;
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

    /// Asks again for the pictures that could not be had and whose wait is over. Cheap when
    /// none is due: called before every frame (`State::tick`).
    pub fn retry_pictures(&mut self) {
        let now = Instant::now();
        if self.avatars.next_retry.is_none_or(|at| at > now) {
            return;
        }
        let mut next: Option<Instant> = None;
        let mut due = Vec::new();
        for (url, picture) in &self.avatars.known {
            if let Picture::Missing { retry: Some(at) } = picture {
                if *at <= now {
                    due.push(url.clone());
                } else {
                    next = Some(next.map_or(*at, |n| n.min(*at)));
                }
            }
        }
        self.avatars.next_retry = next;
        for url in due {
            self.avatars.known.insert(url.clone(), Picture::Wanted);
            self.avatars.queue.push_back(url);
        }
        self.fetch_pictures();
    }

    /// The bytes held for `url` would not decode: they are let go, with the file they were
    /// kept in, and fetched again after a while, as a failed fetch is.
    pub fn picture_failed(&mut self, url: &str) {
        if !matches!(self.avatars.known.get(url), Some(Picture::Held(_))) {
            return;
        }
        if let Some(file) = kept_at(&self.avatars_dir(), url) {
            let _ = std::fs::remove_file(file);
        }
        self.avatars.missed(url, false);
        self.avatars.revision += 1;
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
            let saved = match read_picture(&path) {
                Ok((bytes, media_type)) => app
                    .set_avatar(bytes, media_type)
                    .await
                    .map_err(Failure::from),
                Err(refusal) => Err(refusal),
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
                // An emoji removed while its picture was on its way is not kept.
                if self.avatars.gone.contains(url) {
                    self.fetch_pictures();
                    return;
                }
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
                        self.avatars.tries.remove(url);
                        self.avatars.revision += 1;
                    }
                    // The instance has nothing there: not asked again. Anything else (no
                    // answer, a busy instance) is asked again after a while.
                    Err(failure) => {
                        self.avatars.missed(url, failure.code == "NOT_FOUND");
                        self.avatars.revision += 1;
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
                self.hold_snapshot(snapshot.clone());
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
