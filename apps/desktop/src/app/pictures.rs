//! Pictures as the window draws them. The session holds each avatar as the bytes the
//! instance encoded ([`crate::session::avatars`]); here they become Rune images, decoded on
//! the image cache's own worker and never in a view or a frame. An image that is still
//! decoding, or would not decode, paints nothing, so whatever draws one puts the person's
//! initials under it.

use std::collections::HashMap;
use std::sync::Arc;

use rune::{Image, ImageCache, Waker};

use super::State;
use crate::session::avatars::Avatars;

#[derive(Default)]
pub struct Pictures {
    /// None until the window is set up or a picture is first wanted.
    cache: Option<ImageCache>,
    images: HashMap<String, Image>,
    /// The session's revision these were made from.
    seen: Option<u64>,
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pictures")
            .field("images", &self.images.len())
            .finish_non_exhaustive()
    }
}

impl Pictures {
    /// Starts decoding; `waker` is the window's, called when an image is ready.
    pub fn start(&mut self, waker: Waker) {
        self.cache = Some(ImageCache::new(waker));
        self.seen = None;
    }

    /// Brings the images in line with the bytes the session holds; true when that changed
    /// which there are. Before the window is set up (a scene, a test) the first picture
    /// starts a cache that wakes nobody, which `start` then replaces; a state that is never
    /// given a picture never starts one.
    pub fn sync(&mut self, avatars: &Avatars) -> bool {
        if self.seen == Some(avatars.revision) {
            return false;
        }
        self.seen = Some(avatars.revision);
        let mut images = HashMap::new();
        for (url, bytes) in avatars.held() {
            let image = match self.images.remove(url) {
                Some(image) => image,
                None => self
                    .cache
                    .get_or_insert_with(|| ImageCache::new(Arc::new(|| {})))
                    .load(bytes.clone()),
            };
            images.insert(url.to_string(), image);
        }
        let changed = !self.images.is_empty() || !images.is_empty();
        self.images = images;
        changed
    }

    /// Waits for every image to finish decoding, for a scene drawn in one frame. A window
    /// never waits: an image paints when it is ready.
    pub fn wait(&self) {
        for image in self.images.values() {
            let _ = image.wait();
        }
    }

    /// The image for an avatar URL, when its bytes are held and a window is there to draw.
    pub fn get(&self, url: &str) -> Option<&Image> {
        self.images.get(url)
    }
}

impl State {
    /// The picture of an account, when it has one this computer holds.
    pub fn picture_of(&self, account_id: &str) -> Option<&Image> {
        self.pictures.get(self.session.picture_of(account_id)?)
    }
}
