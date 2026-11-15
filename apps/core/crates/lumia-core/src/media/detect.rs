//! What a file is, told by its first bytes and never by its name, and a
//! picture's declared size, read from its header without decoding a pixel
//! (specs/media.md 3 and 5).

use super::bmff;
use super::bytes::{be16, be32, le16, le24, le32};

/// The longest side a picture may declare before anyone decodes it.
pub const MAX_SIDE: u32 = 8192;
/// The most pixels a picture may declare before anyone decodes it.
pub const MAX_PIXELS: u64 = 40_000_000;

/// A format the core tells apart. Everything else is [`Format::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Jpeg,
    Png,
    Gif,
    WebP,
    Heic,
    Heif,
    Avif,
    Mp4,
    Mov,
    WebM,
    Mp3,
    M4a,
    Aac,
    Ogg,
    Flac,
    Wav,
    Unknown,
}

impl Format {
    /// The media type an attachment of this format carries. `None` for
    /// [`Format::Unknown`], whose type comes from its name (or is
    /// `application/octet-stream`).
    #[must_use]
    pub fn media_type(self) -> Option<&'static str> {
        Some(match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Heic => "image/heic",
            Self::Heif => "image/heif",
            Self::Avif => "image/avif",
            Self::Mp4 => "video/mp4",
            Self::Mov => "video/quicktime",
            Self::WebM => "video/webm",
            Self::Mp3 => "audio/mpeg",
            Self::M4a => "audio/mp4",
            Self::Aac => "audio/aac",
            Self::Ogg => "audio/ogg",
            Self::Flac => "audio/flac",
            Self::Wav => "audio/wav",
            Self::Unknown => return None,
        })
    }

    /// A picture format whose header [`probe`] reads.
    #[must_use]
    pub fn is_picture(self) -> bool {
        matches!(
            self,
            Self::Jpeg | Self::Png | Self::Gif | Self::WebP | Self::Heic | Self::Heif | Self::Avif
        )
    }

    /// An audio format (M4A included).
    #[must_use]
    pub fn is_audio(self) -> bool {
        matches!(
            self,
            Self::Mp3 | Self::M4a | Self::Aac | Self::Ogg | Self::Flac | Self::Wav
        )
    }
}

/// A picture's format and the size its header declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    pub format: Format,
    pub width: u32,
    pub height: u32,
}

impl Probe {
    /// Whether the declared size is within the bounds (see [`within_bounds`]).
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        within_bounds(self.width, self.height)
    }
}

/// The bounds every decode of an attachment is held to, by the sharer and
/// by every receiver: neither side zero, at most [`MAX_SIDE`] a side and
/// [`MAX_PIXELS`] in all. A picture outside them is shown as a file row.
#[must_use]
pub fn within_bounds(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_SIDE
        && height <= MAX_SIDE
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

/// The format of `bytes`, from their first bytes alone.
#[must_use]
pub fn detect(bytes: &[u8]) -> Format {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(&[0xFF, 0xD8, 0xFF]) {
        Format::Jpeg
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        Format::Png
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Format::Gif
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Format::WebP
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        Format::Wav
    } else if starts(b"OggS") {
        Format::Ogg
    } else if starts(b"fLaC") {
        Format::Flac
    } else if starts(b"ID3") {
        Format::Mp3
    } else if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        // EBML: WebM names itself in the header's DocType; any other
        // Matroska is a plain file.
        let head = &bytes[..bytes.len().min(64)];
        if head.windows(4).any(|w| w == b"webm") {
            Format::WebM
        } else {
            Format::Unknown
        }
    } else if bytes.get(4..8) == Some(b"ftyp") {
        ftyp(bytes)
    } else if matches!(bytes.get(4..8), Some(b"moov" | b"wide" | b"mdat" | b"pnot")) {
        // A QuickTime file from before `ftyp`.
        Format::Mov
    } else if mpeg_audio_frame(bytes) {
        Format::Mp3
    } else if adts_frame(bytes) {
        Format::Aac
    } else {
        Format::Unknown
    }
}

/// An ISO BMFF file by its brands: the major brand first, then the
/// compatible ones for the HEIF family.
fn ftyp(bytes: &[u8]) -> Format {
    const HEIC: [&[u8; 4]; 6] = [b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx"];
    const AVIF: [&[u8; 4]; 2] = [b"avif", b"avis"];
    let size = be32(bytes, 0).map_or(0, |s| s as usize).min(bytes.len());
    let Some(major) = bytes.get(8..12) else {
        return Format::Unknown;
    };
    let compatible = || bytes.get(16..size).unwrap_or(&[]).chunks_exact(4);
    let is = |set: &[&[u8; 4]], brand: &[u8]| set.iter().any(|b| b.as_slice() == brand);
    if is(&HEIC, major) {
        Format::Heic
    } else if is(&AVIF, major) {
        Format::Avif
    } else if major == b"mif1" || major == b"msf1" {
        if compatible().any(|b| is(&HEIC, b)) {
            Format::Heic
        } else if compatible().any(|b| is(&AVIF, b)) {
            Format::Avif
        } else {
            Format::Heif
        }
    } else if major == b"qt  " {
        Format::Mov
    } else if major == b"M4A " || major == b"M4B " {
        Format::M4a
    } else if major == b"crx " {
        // Canon raw: ISO BMFF, but no video.
        Format::Unknown
    } else {
        Format::Mp4
    }
}

/// An MPEG-1/2 audio frame header (MP3 without an ID3 tag).
fn mpeg_audio_frame(bytes: &[u8]) -> bool {
    match bytes {
        [0xFF, b1, b2, ..] => {
            b1 & 0xE0 == 0xE0
                && b1 & 0x18 != 0x08 // version: reserved
                && b1 & 0x06 != 0 // layer: reserved
                && b2 >> 4 != 0x0F // bitrate: bad
                && (b2 >> 2) & 0x03 != 0x03 // sample rate: reserved
        }
        _ => false,
    }
}

/// An AAC ADTS frame header.
fn adts_frame(bytes: &[u8]) -> bool {
    match bytes {
        [0xFF, b1, b2, ..] => b1 & 0xF6 == 0xF0 && (b2 >> 2) & 0x0F < 13,
        _ => false,
    }
}

/// A picture's format and declared size, read from its header without
/// decoding: what a receiver checks with [`Probe::within_bounds`] before it
/// decodes anything. `None` when `bytes` are not a picture or their header
/// does not say.
#[must_use]
pub fn probe(bytes: &[u8]) -> Option<Probe> {
    let format = detect(bytes);
    let (width, height) = match format {
        Format::Jpeg => jpeg_size(bytes)?,
        Format::Png => {
            if bytes.get(12..16)? != b"IHDR" {
                return None;
            }
            (be32(bytes, 16)?, be32(bytes, 20)?)
        }
        Format::Gif => (u32::from(le16(bytes, 6)?), u32::from(le16(bytes, 8)?)),
        Format::WebP => webp_size(bytes)?,
        Format::Heic | Format::Heif | Format::Avif => bmff::heif_size(bytes)?,
        _ => return None,
    };
    Some(Probe {
        format,
        width,
        height,
    })
}

/// The size in a JPEG's frame header (any SOFn).
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 2;
    loop {
        if *bytes.get(pos)? != 0xFF {
            return None;
        }
        // Fill bytes before a marker.
        while *bytes.get(pos + 1)? == 0xFF {
            pos += 1;
        }
        let marker = *bytes.get(pos + 1)?;
        pos += 2;
        if marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = usize::from(be16(bytes, pos)?);
        if len < 2 {
            return None;
        }
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let height = u32::from(be16(bytes, pos + 3)?);
            let width = u32::from(be16(bytes, pos + 5)?);
            return Some((width, height));
        }
        pos += len;
    }
}

/// The size in a WebP's first chunk: lossy, lossless or extended.
fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let data = 20;
    match bytes.get(12..16)? {
        b"VP8 " => {
            if bytes.get(data + 3..data + 6)? != [0x9D, 0x01, 0x2A] {
                return None;
            }
            let width = le16(bytes, data + 6)? & 0x3FFF;
            let height = le16(bytes, data + 8)? & 0x3FFF;
            Some((u32::from(width), u32::from(height)))
        }
        b"VP8L" => {
            if *bytes.get(data)? != 0x2F {
                return None;
            }
            let bits = le32(bytes, data + 1)?;
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        b"VP8X" => Some((le24(bytes, data + 4)? + 1, le24(bytes, data + 7)? + 1)),
        _ => None,
    }
}
