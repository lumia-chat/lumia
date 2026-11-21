//! Pictures as the window draws them. The session holds each avatar as the bytes the
//! instance encoded ([`crate::session::avatars`]); here they become Rune images, decoded on
//! the image cache's own worker and never in a view or a frame. An image that is still
//! decoding, or would not decode, paints nothing, so whatever draws one puts the person's
//! initials under it.
//!
//! A server's emoji are decoded only when something draws them, not when their bytes
//! arrive: a server may hold fifty, animated ones among them, and Rune keeps every frame of
//! an animation decoded with its mips (a 128-pixel GIF of 60 frames is about 5 MiB). A view
//! asks for one through [`Pictures::emoji`]; an animation is handed to the cache there
//! (which only queues it, and gives back the one already decoded for the same bytes while
//! anything still draws it), and a first frame, which is decoded here from the GIF with the
//! `image` crate, is made before the next frame ([`Pictures::sync_emoji`], which wakes the
//! window for it). An animation nothing has asked for lately is let go ([`KEEP_FOR`],
//! [`KEEP_MOVING`]), and first frames and still pictures past [`KEEP_STILLS`]; an element still
//! drawing one keeps its pixels until it stops.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rune::{Image, ImageCache, Waker};

use super::State;
use crate::session::avatars::Avatars;

/// How long a decoded emoji nothing has asked for is kept, for a picker opened again or a
/// row drawn again.
pub const KEEP_FOR: Duration = Duration::from_secs(10);
/// How many decoded animations nothing has asked for since the last frame are kept at most:
/// each holds all its frames.
pub const KEEP_MOVING: usize = 4;
/// How many first frames and still pictures nothing has asked for since the last frame are
/// kept at most (a 128-pixel one is under 100 KB).
pub const KEEP_STILLS: usize = 64;

/// One of a server's emoji decoded: its whole picture (an animation's every frame), or the
/// first frame alone (`still`), which Reduce Motion and a message's text draw.
struct EmojiImage {
    /// None when the first frame would not decode.
    image: Option<Image>,
    /// Whether this is an animation's every frame, which is what costs.
    moving: bool,
    /// The bytes it was made from: once the session holds others for its URL (fetched
    /// again), it is made again.
    bytes: Arc<[u8]>,
    used: Instant,
}

#[derive(Default)]
pub struct Pictures {
    /// Empty until the window is set up or a picture is first wanted.
    cache: OnceCell<ImageCache>,
    images: HashMap<String, Image>,
    /// The session's revision these were made from.
    seen: Option<u64>,
    /// The servers' emoji decoded, by URL and whether it is the first frame alone.
    emoji: RefCell<HashMap<(String, bool), EmojiImage>>,
    /// First frames a view asked for that are not made yet.
    stills_wanted: RefCell<HashSet<String>>,
    /// The window's waker, and whether a view has called it since the last frame.
    waker: Option<Waker>,
    woken: Cell<bool>,
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pictures")
            .field("images", &self.images.len())
            .field("emoji", &self.emoji.borrow().len())
            .finish_non_exhaustive()
    }
}

/// What a view draws for one of a server's emoji.
pub enum EmojiFace {
    /// Its picture, painted once decoded.
    Picture(Image),
    /// Nothing yet: its room is kept.
    Waiting,
    /// Its `:name:`: the picture could not be fetched or would not decode (it is asked for
    /// again later).
    Name,
}

/// The first frame of a GIF as straight RGBA, or none when it is not one that decodes.
pub fn first_frame(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let frame = decoder.into_frames().next()?.ok()?;
    let buffer = frame.into_buffer();
    Some((buffer.width(), buffer.height(), buffer.into_raw()))
}

impl Pictures {
    /// Starts decoding; `waker` is the window's, called when an image is ready.
    pub fn start(&mut self, waker: Waker) {
        self.cache = OnceCell::from(ImageCache::new(Arc::clone(&waker)));
        self.waker = Some(waker);
        self.seen = None;
    }

    /// Brings the images of people and servers in line with the bytes the session holds;
    /// true when that changed which there are. The servers' emoji are not among them
    /// ([`Pictures::emoji`]). Before the window is set up (a scene, a test) the first picture
    /// starts a cache that wakes nobody, which `start` then replaces; a state that is never
    /// given a picture never starts one.
    pub fn sync(&mut self, avatars: &Avatars) -> bool {
        if self.seen == Some(avatars.revision) {
            return false;
        }
        self.seen = Some(avatars.revision);
        let mut images = HashMap::new();
        for (url, bytes) in avatars.held().filter(|(url, _)| !avatars.is_emoji(url)) {
            let image = match self.images.remove(url) {
                Some(image) => image,
                None => self.cache().load(bytes.clone()),
            };
            images.insert(url.to_string(), image);
        }
        let changed = !self.images.is_empty() || !images.is_empty();
        self.images = images;
        changed
    }

    fn cache(&self) -> &ImageCache {
        self.cache.get_or_init(|| ImageCache::new(Arc::new(|| {})))
    }

    /// `bytes` decoded on the cache's worker, kept by whoever asked: a file this computer
    /// chose, drawn before it is sent (an emoji's upload form).
    pub fn decode(&mut self, bytes: Arc<[u8]>) -> Image {
        self.cache().load(bytes)
    }

    /// Pixels made here as an image.
    pub fn rgba(&mut self, width: u32, height: u32, rgba: Vec<u8>) -> Image {
        self.cache().rgba(width, height, rgba)
    }

    /// One of the servers' emoji for a view to draw, its bytes `bytes` when held: the whole
    /// picture, or the first frame alone (`still`). The whole picture is handed to the cache
    /// at once; a first frame not made yet is noted for [`Pictures::sync_emoji`] and the
    /// window woken for it. None while there is nothing to draw yet; `Some(None)` when the
    /// first frame would not decode.
    pub fn emoji(
        &self,
        url: &str,
        still: bool,
        animated: bool,
        bytes: Option<&Arc<[u8]>>,
    ) -> Option<Option<Image>> {
        let key = (url.to_string(), still);
        let mut held = self.emoji.borrow_mut();
        let bytes = bytes?;
        if let Some(e) = held.get_mut(&key)
            && Arc::ptr_eq(&e.bytes, bytes)
        {
            e.used = Instant::now();
            return Some(e.image.clone());
        }
        if still {
            if self.stills_wanted.borrow_mut().insert(url.to_string())
                && !self.woken.replace(true)
                && let Some(wake) = &self.waker
            {
                wake();
            }
            return None;
        }
        // The cache only queues it; one still drawn comes back decoded.
        let image = self.cache().load(Arc::clone(bytes));
        held.insert(
            key,
            EmojiImage {
                image: Some(image.clone()),
                moving: animated,
                bytes: Arc::clone(bytes),
                used: Instant::now(),
            },
        );
        Some(Some(image))
    }

    /// Before a frame: makes the first frames views asked for, lets go of what nothing has
    /// asked for lately and of what the session no longer holds, and says which pictures
    /// would not decode (the session lets their bytes go and fetches them again). True when
    /// a first frame was made, which what drew its room needs to draw again for.
    pub fn sync_emoji(&mut self, avatars: &Avatars) -> (bool, Vec<String>) {
        self.woken.set(false);
        let wanted: Vec<String> = self.stills_wanted.borrow_mut().drain().collect();
        let mut made = false;
        let now = Instant::now();
        for url in wanted {
            let Some(bytes) = avatars.bytes_of(&url) else {
                continue;
            };
            let image = first_frame(bytes).map(|(w, h, rgba)| self.cache().rgba(w, h, rgba));
            let bytes = Arc::clone(bytes);
            self.emoji.get_mut().insert(
                (url, true),
                EmojiImage {
                    image,
                    moving: false,
                    bytes,
                    used: now,
                },
            );
            made = true;
        }
        let held = self.emoji.get_mut();
        // Would not decode: the bytes are the session's to let go and fetch again.
        let failed: Vec<String> = held
            .iter()
            .filter(|(_, e)| e.image.as_ref().is_none_or(|i| i.error().is_some()))
            .map(|((url, _), _)| url.clone())
            .collect();
        held.retain(|(url, _), e| {
            avatars
                .bytes_of(url)
                .is_some_and(|now| Arc::ptr_eq(now, &e.bytes))
                && !failed.contains(url)
        });
        // What nothing asked for since the last frame: kept a while, and only so many.
        let mut idle: Vec<((String, bool), bool, Instant)> = held
            .iter()
            .filter(|(_, e)| now.duration_since(e.used) > Duration::from_millis(250))
            .map(|(key, e)| (key.clone(), e.moving, e.used))
            .collect();
        idle.sort_by_key(|(_, _, used)| std::cmp::Reverse(*used));
        let (mut moving, mut stills) = (0, 0);
        for (key, animation, used) in idle {
            let kept = if animation {
                moving += 1;
                moving <= KEEP_MOVING
            } else {
                stills += 1;
                stills <= KEEP_STILLS
            };
            // A first frame or a still picture is kept by count alone: dropped after a quiet
            // while, a list's rows drawn again would show its room empty for a frame.
            if !kept || (animation && now.duration_since(used) > KEEP_FOR) {
                held.remove(&key);
            }
        }
        (made, failed)
    }

    /// Waits for every image to finish decoding, for a scene drawn in one frame. A window
    /// never waits: an image paints when it is ready.
    pub fn wait(&self) {
        for image in self.images.values() {
            let _ = image.wait();
        }
        for image in self
            .emoji
            .borrow()
            .values()
            .filter_map(|e| e.image.as_ref())
        {
            let _ = image.wait();
        }
    }

    /// The image for an avatar URL, when its bytes are held and a window is there to draw.
    pub fn get(&self, url: &str) -> Option<&Image> {
        self.images.get(url)
    }

    /// How many of the servers' emoji are decoded: whole pictures and first frames.
    pub fn emoji_decoded(&self) -> (usize, usize) {
        let held = self.emoji.borrow();
        let stills = held.keys().filter(|(_, still)| *still).count();
        (held.len() - stills, stills)
    }
}

impl State {
    /// The picture a URL names (a server's), when this computer holds it.
    pub fn picture_at(&self, url: &str) -> Option<&Image> {
        if url.is_empty() {
            return None;
        }
        self.pictures.get(url)
    }

    /// The picture of an account, when it has one this computer holds.
    pub fn picture_of(&self, account_id: &str) -> Option<&Image> {
        self.pictures.get(self.session.picture_of(account_id)?)
    }
}
