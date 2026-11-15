//! Attachments before they are uploaded and after they are fetched
//! (docs/specs/media.md 1 to 3): pure functions over bytes, the same on
//! every platform and in wasm.
//!
//! * [`process`] turns a file the sharer chose (and, where the platform
//!   made them, decoded pixels or a poster frame) into the attachment's
//!   metadata and the plaintext of its blobs.
//! * [`seal`] encrypts one blob with a fresh key; [`open`] decrypts a
//!   fetched one and checks its hash.
//! * [`probe`] and [`within_bounds`] are what a receiver checks before it
//!   decodes a picture someone else chose (spec 5).
//!
//! Every decoder in here is Rust; nothing links C, so the core builds for
//! wasm32 as it is.

mod bmff;
mod bytes;
mod detect;
mod still;
mod strip;

#[cfg(test)]
mod tests;

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

pub use crate::proto::AttachmentKind;
pub use detect::{detect, probe, within_bounds, Format, Probe, MAX_PIXELS, MAX_SIDE};

/// What sealing adds to a blob: the Poly1305 tag. A blob's stored size is
/// its plaintext's plus this.
pub const TAG_BYTES: usize = 16;
/// The most a sealed thumbnail may be.
pub const THUMBNAIL_MAX_BYTES: u64 = 256 * 1024;
/// The longest a name may be, in bytes of UTF-8.
pub const NAME_MAX_BYTES: usize = 255;
/// The most attachments one message carries (spec 2).
pub const MAX_ATTACHMENTS: usize = 10;

/// Why an attachment could not be made or a blob could not be opened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MediaError {
    /// The picture does not fit under the image cap even at 640 px.
    #[error("this picture is too large to send, even made smaller")]
    ImageTooLarge { cap: u64 },
    /// The file is over the file cap.
    #[error("this file is {size} bytes, over the limit of {cap}")]
    FileTooLarge { size: u64, cap: u64 },
    /// No thumbnail of the picture fits in 256 KiB.
    #[error("no thumbnail of this picture fits")]
    ThumbnailTooLarge,
    /// Pixels handed in do not match their width and height.
    #[error("the pixels do not match their dimensions")]
    Pixels,
    /// The encoder refused the pixels.
    #[error("the picture could not be encoded: {0}")]
    Encode(String),
    /// The system's randomness source is unavailable.
    #[error("no randomness is available")]
    Randomness,
    /// A key, nonce or hash of the wrong length.
    #[error("the blob's key, nonce or hash is malformed")]
    Malformed,
    /// The ciphertext does not authenticate under its key and nonce.
    #[error("the blob does not open with its key")]
    Decrypt,
    /// The plaintext is not what the hash names.
    #[error("the blob is not the one its message names")]
    HashMismatch,
}

// ---------------------------------------------------------------------------
// Blobs

/// One blob sealed for upload: what goes to the blob store, and what goes
/// in the `BlobRef` that opens it.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedBlob {
    pub ciphertext: Vec<u8>,
    pub key: [u8; 32],
    pub nonce: [u8; 24],
    /// BLAKE3-256 of the plaintext.
    pub hash: [u8; 32],
}

impl std::fmt::Debug for SealedBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedBlob")
            .field("ciphertext_len", &self.ciphertext.len())
            .field("key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// The stored size of a blob whose plaintext is `plaintext_len` bytes.
#[must_use]
pub fn sealed_len(plaintext_len: usize) -> u64 {
    (plaintext_len as u64).saturating_add(TAG_BYTES as u64)
}

/// Seals `plaintext` with a fresh random key and nonce: XChaCha20-Poly1305
/// over the whole plaintext, and the BLAKE3-256 of the plaintext.
///
/// # Errors
/// When randomness is unavailable.
pub fn seal(plaintext: &[u8]) -> Result<SealedBlob, MediaError> {
    let mut key = [0u8; 32];
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut key).map_err(|_| MediaError::Randomness)?;
    getrandom::fill(&mut nonce).map_err(|_| MediaError::Randomness)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| MediaError::Malformed)?;
    let xnonce = XNonce::try_from(&nonce[..]).map_err(|_| MediaError::Malformed)?;
    let ciphertext = cipher
        .encrypt(&xnonce, plaintext)
        .map_err(|_| MediaError::Encode("sealing failed".into()))?;
    Ok(SealedBlob {
        ciphertext,
        key,
        nonce,
        hash: *blake3::hash(plaintext).as_bytes(),
    })
}

/// Opens a fetched blob with the key, nonce and hash its `BlobRef` names,
/// and checks the plaintext against the hash.
///
/// # Errors
/// [`MediaError::Malformed`] for a key, nonce or hash of the wrong length,
/// [`MediaError::Decrypt`] when the ciphertext does not authenticate, and
/// [`MediaError::HashMismatch`] when the plaintext is not what the hash
/// names.
pub fn open(
    ciphertext: &[u8],
    key: &[u8],
    nonce: &[u8],
    hash: &[u8],
) -> Result<Vec<u8>, MediaError> {
    if hash.len() != 32 {
        return Err(MediaError::Malformed);
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| MediaError::Malformed)?;
    let xnonce = XNonce::try_from(nonce).map_err(|_| MediaError::Malformed)?;
    let plaintext = cipher
        .decrypt(&xnonce, ciphertext)
        .map_err(|_| MediaError::Decrypt)?;
    if blake3::hash(&plaintext).as_bytes() != hash {
        return Err(MediaError::HashMismatch);
    }
    Ok(plaintext)
}

/// Whether `plaintext` is what `hash` (a `BlobRef`'s BLAKE3-256) names: for a
/// blob kept after it was opened, checked again when it is read back.
#[must_use]
pub fn hash_matches(plaintext: &[u8], hash: &[u8]) -> bool {
    blake3::hash(plaintext).as_bytes()[..] == *hash
}

// ---------------------------------------------------------------------------
// Processing

/// The instance's caps on a main blob, in sealed bytes (spec 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// An `IMAGE`'s display blob (`LUMIA_UPLOAD_IMAGE_MAX_BYTES`).
    pub image_bytes: u64,
    /// Every other main blob (`LUMIA_UPLOAD_FILE_MAX_BYTES`).
    pub file_bytes: u64,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            image_bytes: 2 * 1024 * 1024,
            file_bytes: 10 * 1024 * 1024,
        }
    }
}

/// Decoded pixels from the platform: RGBA8, row by row, already upright.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pixels")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// A file the sharer chose, and what the platform knows of it.
#[derive(Clone, Default)]
pub struct Input {
    pub bytes: Vec<u8>,
    /// The file's name as the platform gave it.
    pub name: String,
    /// Sent "as a file": no resizing, no re-encoding, metadata stripped
    /// losslessly where spec 3.4 says.
    pub as_file: bool,
    /// The picture decoded by the platform, for a format the core does not
    /// decode (HEIC, HEIF). When present, the attachment is an `IMAGE` made
    /// from these pixels, whatever the bytes are.
    pub pixels: Option<Pixels>,
    /// A video's poster frame, decoded by the platform.
    pub poster: Option<Pixels>,
    /// A video's or audio's length, when the platform read it. The core
    /// prefers the container's for MP4, MOV and M4A.
    pub duration_ms: Option<u32>,
    /// A video's width and height after rotation, when the platform read
    /// them; used where the core cannot read the container (WebM). Without
    /// it a WebM takes its poster's size.
    pub video_size: Option<(u32, u32)>,
    pub caps: Caps,
}

impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("bytes_len", &self.bytes.len())
            .field("name", &self.name)
            .field("as_file", &self.as_file)
            .field("pixels", &self.pixels)
            .field("poster", &self.poster)
            .field("duration_ms", &self.duration_ms)
            .field("video_size", &self.video_size)
            .field("caps", &self.caps)
            .finish()
    }
}

/// An attachment ready to seal and send: its metadata and the plaintext of
/// its blobs.
#[derive(Clone, PartialEq, Eq)]
pub struct Processed {
    pub kind: AttachmentKind,
    pub media_type: String,
    /// At most [`NAME_MAX_BYTES`] of UTF-8, cut on a character boundary.
    /// An `IMAGE`'s extension names what the core made (`.jpg`, `.png`).
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// 0 when unknown.
    pub duration_ms: u32,
    /// ThumbHash of the picture, poster or first frame; empty when there
    /// is none.
    pub placeholder: Vec<u8>,
    pub main: Vec<u8>,
    pub thumbnail: Option<Vec<u8>>,
}

impl std::fmt::Debug for Processed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Processed")
            .field("kind", &self.kind)
            .field("media_type", &self.media_type)
            .field("name", &self.name)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("duration_ms", &self.duration_ms)
            .field("placeholder_len", &self.placeholder.len())
            .field("main_len", &self.main.len())
            .field("thumbnail_len", &self.thumbnail.as_ref().map(Vec::len))
            .finish()
    }
}

/// Makes an attachment of a file (specs/media.md 3):
///
/// * Platform pixels, or a JPEG, PNG, WebP or single-frame GIF that
///   decodes within the bounds: an `IMAGE`, re-encoded from its pixels.
/// * An animated GIF: a `GIF`, byte for byte but its comment and
///   application extensions, with its first frame as the thumbnail.
/// * MP4 and MOV: a `VIDEO` with its metadata boxes freed and its times
///   zeroed; WebM as chosen. A poster frame becomes the thumbnail.
/// * Audio (M4A's metadata freed as MP4's): an `AUDIO`.
/// * Anything else, a picture sent as a file, and a picture that fails the
///   bounds or will not decode: a `FILE`, with the lossless strip of 3.4.
///
/// # Errors
/// When the platform's pixels do not match their size, a picture cannot
/// fit under the image cap at 640 px, or a main blob is over the file cap.
pub fn process(input: Input) -> Result<Processed, MediaError> {
    let Input {
        bytes,
        name,
        as_file,
        pixels,
        poster,
        duration_ms,
        video_size,
        caps,
    } = input;
    let format = detect(&bytes);
    let image_cap = caps.image_bytes;

    if !as_file {
        if let Some(pixels) = pixels {
            let image = still::from_pixels(pixels.rgba, pixels.width, pixels.height)?;
            return image_attachment(still::display(still::shrink(image), image_cap)?, &name);
        }
        match format {
            Format::Jpeg | Format::Png | Format::WebP => {
                if let Some(image) = still::decode(&bytes) {
                    let shown = still::shrink_decoded(&image);
                    drop(image);
                    return image_attachment(still::display(shown, image_cap)?, &name);
                }
            }
            Format::Gif => {
                if let Some(made) = gif(&bytes, &name, caps)? {
                    return Ok(made);
                }
            }
            Format::Mp4 | Format::Mov | Format::M4a => {
                return movie(bytes, format, &name, poster, duration_ms, caps);
            }
            Format::WebM => {
                let preview = poster_preview(poster.as_ref())?;
                let size = video_size.or_else(|| poster.as_ref().map(|p| (p.width, p.height)));
                return finish(
                    Processed {
                        kind: AttachmentKind::Video,
                        media_type: "video/webm".into(),
                        name: clip_name(&name),
                        width: size.map_or(0, |s| s.0),
                        height: size.map_or(0, |s| s.1),
                        duration_ms: duration_ms.unwrap_or(0),
                        placeholder: preview
                            .as_ref()
                            .map(|p| p.placeholder.clone())
                            .unwrap_or_default(),
                        main: bytes,
                        thumbnail: preview.map(|p| p.thumbnail),
                    },
                    caps,
                );
            }
            f if f.is_audio() => {
                return finish(
                    Processed {
                        kind: AttachmentKind::Audio,
                        media_type: media_type(f, &name),
                        name: clip_name(&name),
                        width: 0,
                        height: 0,
                        duration_ms: duration_ms.unwrap_or(0),
                        placeholder: Vec::new(),
                        main: bytes,
                        thumbnail: None,
                    },
                    caps,
                );
            }
            _ => {}
        }
    }
    file(bytes, format, &name, caps)
}

/// An `IMAGE` from a display picture.
fn image_attachment(made: still::Still, name: &str) -> Result<Processed, MediaError> {
    let (extension, media_type) = if made.png {
        ("png", "image/png")
    } else {
        ("jpg", "image/jpeg")
    };
    Ok(Processed {
        kind: AttachmentKind::Image,
        media_type: media_type.into(),
        name: rename(name, extension),
        width: made.width,
        height: made.height,
        duration_ms: 0,
        placeholder: made.placeholder,
        main: made.main,
        thumbnail: Some(made.thumbnail),
    })
}

/// A GIF: animated, a `GIF`; one frame, an `IMAGE`. `None` when it will not
/// walk or decode, and goes as a file.
fn gif(bytes: &[u8], name: &str, caps: Caps) -> Result<Option<Processed>, MediaError> {
    let Some((stripped, frames)) = strip::gif(bytes) else {
        return Ok(None);
    };
    let Some(first) = still::decode(&stripped) else {
        return Ok(None);
    };
    if frames == 1 {
        let shown = still::shrink_decoded(&first);
        drop(first);
        return image_attachment(still::display(shown, caps.image_bytes)?, name).map(Some);
    }
    let Some(screen) = probe(&stripped) else {
        return Ok(None);
    };
    let first = first.to_rgba8();
    let png = first.pixels().any(|p| p.0[3] < 255);
    let preview = still::preview(&image::DynamicImage::ImageRgba8(first), png)?;
    finish(
        Processed {
            kind: AttachmentKind::Gif,
            media_type: "image/gif".into(),
            name: clip_name(name),
            width: screen.width,
            height: screen.height,
            duration_ms: 0,
            placeholder: preview.placeholder,
            main: stripped,
            thumbnail: Some(preview.thumbnail),
        },
        caps,
    )
    .map(Some)
}

/// An MP4, MOV or M4A, its metadata wiped in place: a `VIDEO` when it has
/// a video track, an `AUDIO` when it has only sound, and a `FILE` when its
/// boxes do not hold together.
fn movie(
    mut bytes: Vec<u8>,
    format: Format,
    name: &str,
    poster: Option<Pixels>,
    duration_ms: Option<u32>,
    caps: Caps,
) -> Result<Processed, MediaError> {
    let Some(movie) = bmff::wipe_movie(&mut bytes) else {
        return file(bytes, format, name, caps);
    };
    let duration_ms = if movie.duration_ms > 0 {
        movie.duration_ms
    } else {
        duration_ms.unwrap_or(0)
    };
    if format == Format::M4a || (!movie.has_video && movie.has_audio) {
        return finish(
            Processed {
                kind: AttachmentKind::Audio,
                media_type: "audio/mp4".into(),
                name: clip_name(name),
                width: 0,
                height: 0,
                duration_ms,
                placeholder: Vec::new(),
                main: bytes,
                thumbnail: None,
            },
            caps,
        );
    }
    let preview = poster_preview(poster.as_ref())?;
    let (width, height) = if movie.has_video {
        (movie.width, movie.height)
    } else {
        poster.as_ref().map_or((0, 0), |p| (p.width, p.height))
    };
    finish(
        Processed {
            kind: AttachmentKind::Video,
            media_type: media_type(format, name),
            name: clip_name(name),
            width,
            height,
            duration_ms,
            placeholder: preview
                .as_ref()
                .map(|p| p.placeholder.clone())
                .unwrap_or_default(),
            main: bytes,
            thumbnail: preview.map(|p| p.thumbnail),
        },
        caps,
    )
}

/// A poster frame's thumbnail and placeholder.
fn poster_preview(poster: Option<&Pixels>) -> Result<Option<still::Preview>, MediaError> {
    let Some(poster) = poster else {
        return Ok(None);
    };
    let image = still::from_pixels(poster.rgba.clone(), poster.width, poster.height)?;
    let png = image.pixels().any(|p| p.0[3] < 255);
    still::preview(&image::DynamicImage::ImageRgba8(image), png).map(Some)
}

/// A `FILE`: pictures and movies lose their metadata losslessly (spec
/// 3.4); anything else, or a file whose structure does not hold together,
/// goes as chosen.
fn file(
    mut bytes: Vec<u8>,
    format: Format,
    name: &str,
    caps: Caps,
) -> Result<Processed, MediaError> {
    let stripped = match format {
        Format::Jpeg => strip::jpeg(&bytes),
        Format::Png => strip::png(&bytes),
        Format::WebP => strip::webp(&bytes),
        Format::Gif => strip::gif(&bytes).map(|(b, _)| b),
        Format::Heic | Format::Heif | Format::Avif => {
            bmff::wipe_heif(&mut bytes);
            None
        }
        Format::Mp4 | Format::Mov | Format::M4a => {
            bmff::wipe_movie(&mut bytes);
            None
        }
        _ => None,
    };
    let main = stripped.unwrap_or(bytes);
    finish(
        Processed {
            kind: AttachmentKind::File,
            media_type: media_type(format, name),
            name: clip_name(name),
            width: 0,
            height: 0,
            duration_ms: 0,
            placeholder: Vec::new(),
            main,
            thumbnail: None,
        },
        caps,
    )
}

/// Checks a non-`IMAGE` main blob against the file cap.
fn finish(processed: Processed, caps: Caps) -> Result<Processed, MediaError> {
    let size = sealed_len(processed.main.len());
    if size > caps.file_bytes {
        return Err(MediaError::FileTooLarge {
            size,
            cap: caps.file_bytes,
        });
    }
    Ok(processed)
}

/// The media type of a format, or for an unknown one a few common types by
/// the name's extension, else `application/octet-stream`.
fn media_type(format: Format, name: &str) -> String {
    if let Some(known) = format.media_type() {
        return known.into();
    }
    let extension = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
    .into()
}

/// The picture a placeholder (an attachment's ThumbHash) stands for: a few
/// dozen pixels a side, RGBA8, for a shell to scale up and draw until the
/// thumbnail arrives. `None` for bytes that are not a ThumbHash.
#[must_use]
pub fn placeholder_image(placeholder: &[u8]) -> Option<Pixels> {
    if placeholder.len() < 5 {
        return None;
    }
    let (width, height, rgba) = thumbhash::thumb_hash_to_rgba(placeholder).ok()?;
    Some(Pixels {
        rgba,
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
    })
}

/// `name` cut to at most [`NAME_MAX_BYTES`] on a character boundary.
#[must_use]
pub fn clip_name(name: &str) -> String {
    clip(name, NAME_MAX_BYTES).to_owned()
}

fn clip(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// `name` with its extension made `extension`, the stem cut so the whole
/// stays within [`NAME_MAX_BYTES`].
fn rename(name: &str, extension: &str) -> String {
    let stem = match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    };
    let stem = if stem.is_empty() { "image" } else { stem };
    let stem = clip(stem, NAME_MAX_BYTES - 1 - extension.len());
    format!("{stem}.{extension}")
}
