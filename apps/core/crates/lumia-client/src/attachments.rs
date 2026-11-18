//! Attachment blobs over the wire (docs/specs/media.md 4): sealing and
//! uploading what [`lumia_core::media::process`] made, fetching and opening
//! a blob a message names, and keeping what was opened so it is fetched once.
//!
//! The work on the bytes is the core's; this is the I/O around it, shared by
//! [`crate::app::App`] and the CLI.

use std::collections::VecDeque;

use lumia_core::media::{self, AttachmentKind, Processed};
use lumia_core::proto::{Attachment, BlobRef, BlobRole, Card, UploadLimits};
use lumia_core::wire;

use crate::api::Api;
use crate::ClientError;

/// The largest blob fetched at all, whatever a message says of it.
pub const FETCH_MAX_BYTES: u64 = 1 << 30;

/// The caps [`media::process`] keeps to, from the instance's limits.
#[must_use]
pub fn caps(limits: &UploadLimits) -> media::Caps {
    media::Caps {
        image_bytes: limits.image_max_bytes,
        file_bytes: limits.file_max_bytes,
        thumbnail_bytes: limits.thumbnail_max_bytes,
    }
}

/// The role a main blob of `kind` is created for (spec 4.1).
#[must_use]
pub fn main_role(kind: AttachmentKind) -> BlobRole {
    if kind == AttachmentKind::Image {
        BlobRole::Display
    } else {
        BlobRole::File
    }
}

/// Seals `plaintext` and uploads it as a blob of `role` in the conversation
/// `channel_id`: a slot, the ciphertext, the word that it is there. Answers
/// the reference that opens it.
///
/// # Errors
/// When the instance refuses the slot (over the cap, no blob store, no
/// `ATTACH_FILES`), the store refuses the bytes, or the transport fails.
pub async fn upload_blob(
    api: &Api,
    channel_id: &[u8],
    role: BlobRole,
    plaintext: &[u8],
) -> Result<BlobRef, ClientError> {
    let sealed = media::seal(plaintext)?;
    let size_bytes = sealed.ciphertext.len() as u64;
    let slot = api
        .create_upload(channel_id.to_vec(), role, size_bytes)
        .await?;
    let put = api
        .put_upload(&slot.upload_url, &slot.upload_headers, sealed.ciphertext)
        .await;
    if let Err(e) = put.and(api.complete_upload(&slot.blob_id).await) {
        let _ = api.delete_blob(&slot.blob_id).await;
        return Err(e);
    }
    Ok(BlobRef {
        blob_id: slot.blob_id,
        size_bytes,
        key: sealed.key.to_vec(),
        nonce: sealed.nonce.to_vec(),
        hash: sealed.hash.to_vec(),
    })
}

/// Uploads a processed attachment's blobs into `channel_id` and answers the
/// attachment, with a fresh id and its keys, ready to send. `alt` is cut to
/// the protocol's 1,000 characters. A thumbnail (a GIF's or a video's) that
/// fails takes the main blob back with it.
///
/// # Errors
/// As [`upload_blob`].
pub async fn upload(
    api: &Api,
    channel_id: &[u8],
    processed: Processed,
    alt: &str,
) -> Result<Attachment, ClientError> {
    let main = upload_blob(api, channel_id, main_role(processed.kind), &processed.main).await?;
    let thumbnail = match &processed.thumbnail {
        Some(bytes) => match upload_blob(api, channel_id, BlobRole::Thumbnail, bytes).await {
            Ok(blob) => Some(blob),
            Err(e) => {
                let _ = api.delete_blob(&main.blob_id).await;
                return Err(e);
            }
        },
        None => None,
    };
    Ok(Attachment {
        attachment_id: wire::new_id(),
        kind: processed.kind as i32,
        name: processed.name,
        media_type: processed.media_type,
        width: processed.width,
        height: processed.height,
        duration_ms: processed.duration_ms,
        placeholder: processed.placeholder,
        alt: clip_alt(alt),
        main: Some(main),
        thumbnail,
        expired: false,
    })
}

/// The most of an alt text the protocol takes, in characters.
pub const ALT_MAX_CHARS: usize = 1_000;

/// `alt` trimmed and cut to [`ALT_MAX_CHARS`].
#[must_use]
pub fn clip_alt(alt: &str) -> String {
    alt.trim().chars().take(ALT_MAX_CHARS).collect()
}

/// Every blob `attachments` and `cards` use: what `SendEncrypted.blob_ids`
/// lists for a personal message (spec 4.2).
#[must_use]
pub fn blob_ids(attachments: &[Attachment], cards: &[Card]) -> Vec<Vec<u8>> {
    let mut ids: Vec<Vec<u8>> = Vec::new();
    let refs = attachments
        .iter()
        .flat_map(|a| [a.main.as_ref(), a.thumbnail.as_ref()])
        .chain(cards.iter().map(|c| c.image.as_ref()))
        .flatten();
    for blob in refs {
        if !ids.contains(&blob.blob_id) {
            ids.push(blob.blob_id.clone());
        }
    }
    ids
}

/// Fetches a blob's ciphertext and opens it with its reference (spec 4.3).
///
/// # Errors
/// `NOT_FOUND` when this account cannot read a message naming it; a blob
/// that does not open under its key or is not the one its hash names; the
/// transport.
pub async fn fetch(api: &Api, blob: &BlobRef) -> Result<Vec<u8>, ClientError> {
    let limit = blob.size_bytes.min(FETCH_MAX_BYTES);
    let ciphertext = api.blob(&blob.blob_id, limit).await?;
    Ok(media::open(
        &ciphertext,
        &blob.key,
        &blob.nonce,
        &blob.hash,
    )?)
}

// ---- The cache ------------------------------------------------------------------

/// What is kept in memory of opened blobs, at most.
#[cfg(not(target_arch = "wasm32"))]
pub const MEMORY_MAX_BYTES: usize = 32 << 20;
/// What is kept in memory of opened blobs, at most (the web keeps no files).
#[cfg(target_arch = "wasm32")]
pub const MEMORY_MAX_BYTES: usize = 64 << 20;
/// What a cache directory holds, at most: past it the least recently used
/// go first.
pub const DISK_MAX_BYTES: u64 = 512 << 20;

/// Opened blobs, by id: a few in memory, least recently used out first, and
/// natively, once given a directory, every one there up to
/// [`DISK_MAX_BYTES`]. What is read back is checked against the hash the
/// asking message names, so a blob is never mistaken for another.
#[derive(Default)]
pub struct BlobCache {
    memory: VecDeque<(Vec<u8>, Vec<u8>)>,
    memory_bytes: usize,
    #[cfg(not(target_arch = "wasm32"))]
    dir: Option<std::path::PathBuf>,
}

impl std::fmt::Debug for BlobCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlobCache")
            .field("in_memory", &self.memory.len())
            .field("memory_bytes", &self.memory_bytes)
            .finish_non_exhaustive()
    }
}

impl BlobCache {
    /// Keeps opened blobs in `dir` too (created when missing).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_dir(&mut self, dir: std::path::PathBuf) {
        self.dir = Some(dir);
    }

    /// The opened blob `blob` names, when kept.
    pub fn get(&mut self, blob: &BlobRef) -> Option<Vec<u8>> {
        if let Some(at) = self.memory.iter().position(|(id, _)| *id == blob.blob_id) {
            let found = self.memory.remove(at)?;
            if media::hash_matches(&found.1, &blob.hash) {
                let bytes = found.1.clone();
                self.memory.push_back(found);
                return Some(bytes);
            }
            self.memory_bytes -= found.1.len();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(bytes) = self.read_file(blob) {
            self.remember(&blob.blob_id, &bytes);
            return Some(bytes);
        }
        None
    }

    /// Keeps an opened blob.
    pub fn put(&mut self, blob_id: &[u8], plaintext: &[u8]) {
        self.remember(blob_id, plaintext);
        #[cfg(not(target_arch = "wasm32"))]
        self.write_file(blob_id, plaintext);
    }

    fn remember(&mut self, blob_id: &[u8], plaintext: &[u8]) {
        if plaintext.len() > MEMORY_MAX_BYTES / 4 {
            return;
        }
        if let Some(at) = self.memory.iter().position(|(id, _)| id == blob_id) {
            if let Some((_, old)) = self.memory.remove(at) {
                self.memory_bytes -= old.len();
            }
        }
        self.memory
            .push_back((blob_id.to_vec(), plaintext.to_vec()));
        self.memory_bytes += plaintext.len();
        while self.memory_bytes > MEMORY_MAX_BYTES {
            match self.memory.pop_front() {
                Some((_, old)) => self.memory_bytes -= old.len(),
                None => break,
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn path(&self, blob_id: &[u8]) -> Option<std::path::PathBuf> {
        Some(self.dir.as_ref()?.join(hex::encode(blob_id)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_file(&self, blob: &BlobRef) -> Option<Vec<u8>> {
        let path = self.path(&blob.blob_id)?;
        let bytes = std::fs::read(&path).ok()?;
        if !media::hash_matches(&bytes, &blob.hash) {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        // Read now: the last to go when the directory is trimmed.
        if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) {
            let _ = file.set_modified(std::time::SystemTime::now());
        }
        Some(bytes)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn write_file(&self, blob_id: &[u8], plaintext: &[u8]) {
        let (Some(dir), Some(path)) = (self.dir.as_ref(), self.path(blob_id)) else {
            return;
        };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        // Written aside and moved into place, so a reader never meets half of one.
        let partial = path.with_extension("partial");
        if std::fs::write(&partial, plaintext).is_err() || std::fs::rename(&partial, &path).is_err()
        {
            let _ = std::fs::remove_file(&partial);
            return;
        }
        trim_dir(dir, DISK_MAX_BYTES);
    }
}

/// Removes the least recently used files of `dir` until what is left is
/// within nine tenths of `max`, once it holds more than `max`.
#[cfg(not(target_arch = "wasm32"))]
fn trim_dir(dir: &std::path::Path, max: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((meta.modified().ok()?, meta.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= max {
        return;
    }
    files.sort_by_key(|f| f.0);
    for (_, len, path) in files {
        if total <= max / 10 * 9 {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn blob(id: u8, plaintext: &[u8]) -> BlobRef {
        let sealed = media::seal(plaintext).unwrap();
        BlobRef {
            blob_id: vec![id; 16],
            size_bytes: sealed.ciphertext.len() as u64,
            key: sealed.key.to_vec(),
            nonce: sealed.nonce.to_vec(),
            hash: sealed.hash.to_vec(),
        }
    }

    #[test]
    fn every_blob_a_message_uses_is_listed_once() {
        let a = Attachment {
            main: Some(blob(1, b"a")),
            thumbnail: Some(blob(2, b"b")),
            ..Default::default()
        };
        let c = Card {
            image: Some(blob(3, b"c")),
            ..Default::default()
        };
        let plain = Card::default();
        assert_eq!(
            blob_ids(&[a.clone(), a], &[c, plain]),
            vec![vec![1; 16], vec![2; 16], vec![3; 16]]
        );
    }

    #[test]
    fn the_cache_answers_only_for_the_hash_asked_for() {
        let mut cache = BlobCache::default();
        let one = blob(1, b"one");
        cache.put(&one.blob_id, b"one");
        assert_eq!(cache.get(&one), Some(b"one".to_vec()));
        // The same id named with another hash is not this blob.
        let other = blob(1, b"two");
        assert_eq!(cache.get(&other), None);
        assert_eq!(cache.get(&one), None, "a mismatch forgets what was kept");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_cache_directory_keeps_blobs_across_caches_and_is_trimmed_oldest_first() {
        let dir =
            std::env::temp_dir().join(format!("lumia-blobs-{}", wire::display(&wire::new_id())));
        let one = blob(1, b"one");
        {
            let mut cache = BlobCache::default();
            cache.set_dir(dir.clone());
            cache.put(&one.blob_id, b"one");
        }
        let mut cache = BlobCache::default();
        cache.set_dir(dir.clone());
        assert_eq!(cache.get(&one), Some(b"one".to_vec()));

        for (n, age) in [(1u8, 30u64), (2, 20), (3, 10)] {
            let path = dir.join(hex::encode([n; 16]));
            std::fs::write(&path, vec![n; 400]).unwrap();
            let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(at)
                .unwrap();
        }
        trim_dir(&dir, 1000);
        assert!(
            !dir.join(hex::encode([1u8; 16])).exists(),
            "the oldest went"
        );
        assert!(
            dir.join(hex::encode([2u8; 16])).exists(),
            "within nine tenths"
        );
        assert!(
            dir.join(hex::encode([3u8; 16])).exists(),
            "the newest stayed"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
