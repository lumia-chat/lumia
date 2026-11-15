//! ISO base media files (MP4, MOV, M4A, HEIF), edited in place: no box
//! changes size, so no offset anywhere in the file moves (specs/media.md
//! 3.3 and 3.4).

use super::bytes::{be16, be32, be64, be_sized};

/// Adobe's XMP box: a top-level `uuid` box with this extended type.
const XMP_UUID: [u8; 16] = [
    0xBE, 0x7A, 0xCF, 0xCB, 0x97, 0xA9, 0x42, 0xE8, 0x9C, 0x71, 0x99, 0x94, 0x91, 0xE3, 0xAF, 0xAC,
];

/// One box: where it starts, where its payload starts and where it ends.
#[derive(Debug, Clone, Copy)]
struct BoxAt {
    start: usize,
    payload: usize,
    end: usize,
    kind: [u8; 4],
}

/// The box at `pos`, which must end by `limit`.
fn box_at(bytes: &[u8], pos: usize, limit: usize) -> Option<BoxAt> {
    let size = be32(bytes, pos)?;
    let kind: [u8; 4] = bytes.get(pos + 4..pos + 8)?.try_into().ok()?;
    let (payload, end) = match size {
        // To the end of the enclosing box (or file).
        0 => (pos + 8, limit),
        1 => {
            let large = usize::try_from(be64(bytes, pos + 8)?).ok()?;
            (pos + 16, pos.checked_add(large)?)
        }
        size => (pos + 8, pos.checked_add(size as usize)?),
    };
    let payload = if &kind == b"uuid" {
        payload + 16
    } else {
        payload
    };
    (payload <= end && end <= limit).then_some(BoxAt {
        start: pos,
        payload,
        end,
        kind,
    })
}

/// The boxes from `start` to `end`, or `None` when one does not fit.
fn children(bytes: &[u8], start: usize, end: usize) -> Option<Vec<BoxAt>> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos < end {
        // A few trailing bytes too short for a header: tolerated, as
        // players tolerate them.
        if end - pos < 8 {
            break;
        }
        let b = box_at(bytes, pos, end)?;
        pos = b.end;
        out.push(b);
    }
    Some(out)
}

/// What the movie said about itself, read while wiping it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Movie {
    /// The first video track's size, after its rotation.
    pub width: u32,
    pub height: u32,
    /// From `mvhd`; 0 when the container does not know.
    pub duration_ms: u32,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Debug, Default)]
struct Track {
    width: u32,
    height: u32,
    handler: [u8; 4],
}

/// Rewrites an MP4, MOV or M4A in place: every `udta` and `meta` box (and
/// a top-level XMP `uuid` box) becomes a `free` box of the same size with
/// its payload zeroed, and the creation and modification times in `mvhd`,
/// `tkhd` and `mdhd` become zero. `None` when the box structure does not
/// hold together; boxes before the fault may already be wiped, and the
/// file is still the same length.
pub(super) fn wipe_movie(bytes: &mut [u8]) -> Option<Movie> {
    let mut movie = Movie::default();
    let mut tracks = Vec::new();
    let len = bytes.len();
    for b in children(bytes, 0, len)? {
        match &b.kind {
            b"moov" | b"moof" => walk(bytes, b, &mut movie, &mut tracks)?,
            b"udta" | b"meta" => free(bytes, b),
            b"uuid" if bytes.get(b.payload - 16..b.payload) == Some(&XMP_UUID[..]) => {
                free(bytes, b);
            }
            _ => {}
        }
    }
    if let Some(video) = tracks.iter().find(|t| &t.handler == b"vide") {
        movie.has_video = true;
        movie.width = video.width;
        movie.height = video.height;
    }
    movie.has_audio = tracks.iter().any(|t| &t.handler == b"soun");
    Some(movie)
}

/// The boxes inside a container of the movie.
fn walk(bytes: &mut [u8], parent: BoxAt, movie: &mut Movie, tracks: &mut Vec<Track>) -> Option<()> {
    for b in children(bytes, parent.payload, parent.end)? {
        match &b.kind {
            b"udta" | b"meta" => free(bytes, b),
            b"trak" => {
                tracks.push(Track::default());
                walk(bytes, b, movie, tracks)?;
            }
            b"mdia" | b"minf" | b"stbl" | b"edts" | b"dinf" | b"mvex" | b"traf" => {
                walk(bytes, b, movie, tracks)?;
            }
            b"mvhd" => {
                let (timescale, duration) = header_times(bytes, b, true)?;
                if timescale > 0 && duration > 0 && duration != u64::from(u32::MAX) {
                    let ms = u128::from(duration) * 1000 / u128::from(timescale);
                    movie.duration_ms = u32::try_from(ms).unwrap_or(u32::MAX);
                }
            }
            b"mdhd" => {
                header_times(bytes, b, true)?;
            }
            b"tkhd" => {
                header_times(bytes, b, false)?;
                if let (Some(track), Some(size)) = (tracks.last_mut(), track_size(bytes, b)) {
                    (track.width, track.height) = size;
                }
            }
            b"hdlr" if &parent.kind == b"mdia" => {
                if let (Some(track), Some(handler)) =
                    (tracks.last_mut(), bytes.get(b.payload + 8..b.payload + 12))
                {
                    track.handler.copy_from_slice(handler);
                }
            }
            _ => {}
        }
    }
    Some(())
}

/// Turns a box into a `free` box of the same size with its payload zeroed.
fn free(bytes: &mut [u8], b: BoxAt) {
    if let Some(kind) = bytes.get_mut(b.start + 4..b.start + 8) {
        kind.copy_from_slice(b"free");
    }
    // A `uuid` box's extended type is payload to a `free` box.
    let from = if &b.kind == b"uuid" {
        b.payload - 16
    } else {
        b.payload
    };
    if let Some(payload) = bytes.get_mut(from..b.end) {
        payload.fill(0);
    }
}

/// Zeroes the creation and modification times of an `mvhd`, `tkhd` or
/// `mdhd` (a full box: version 1 has 64-bit times, version 0 32-bit), and
/// returns its timescale and duration where it has them (`mvhd`, `mdhd`).
fn header_times(bytes: &mut [u8], b: BoxAt, timescale: bool) -> Option<(u32, u64)> {
    let version = *bytes.get(b.payload)?;
    let at = b.payload + 4;
    let width = if version == 1 { 8 } else { 4 };
    bytes.get_mut(at..at + 2 * width)?.fill(0);
    if !timescale {
        return Some((0, 0));
    }
    let scale = be32(bytes, at + 2 * width)?;
    let duration = be_sized(bytes, at + 2 * width + 4, width)?;
    Some((scale, duration))
}

/// A `tkhd`'s width and height (16.16 fixed point), swapped when its matrix
/// turns the track a quarter.
fn track_size(bytes: &[u8], b: BoxAt) -> Option<(u32, u32)> {
    let version = *bytes.get(b.payload)?;
    // version/flags, times, track id, reserved, duration, reserved[2],
    // layer, alternate group, volume, reserved.
    let matrix = b.payload + if version == 1 { 52 } else { 40 };
    let width = be32(bytes, matrix + 36)? >> 16;
    let height = be32(bytes, matrix + 40)? >> 16;
    let a = be32(bytes, matrix)?;
    let d = be32(bytes, matrix + 16)?;
    Some(if a == 0 && d == 0 {
        (height, width)
    } else {
        (width, height)
    })
}

/// The top-level `meta` box of a HEIF file and its children.
fn heif_meta(bytes: &[u8]) -> Option<(BoxAt, Vec<BoxAt>)> {
    let meta = children(bytes, 0, bytes.len())?
        .into_iter()
        .find(|b| &b.kind == b"meta")?;
    // A full box: version and flags first.
    let inner = children(bytes, meta.payload + 4, meta.end)?;
    Some((meta, inner))
}

/// A HEIF's size from its `ispe` properties: the largest, which is the
/// primary picture (or the grid it is tiled from) rather than a thumbnail.
pub(super) fn heif_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let (_, inner) = heif_meta(bytes)?;
    let iprp = inner.iter().find(|b| &b.kind == b"iprp")?;
    let ipco = children(bytes, iprp.payload, iprp.end)?
        .into_iter()
        .find(|b| &b.kind == b"ipco")?;
    children(bytes, ipco.payload, ipco.end)?
        .into_iter()
        .filter(|b| &b.kind == b"ispe")
        .filter_map(|b| Some((be32(bytes, b.payload + 4)?, be32(bytes, b.payload + 8)?)))
        .max_by_key(|&(w, h)| u64::from(w) * u64::from(h))
}

/// Zeroes, in place, the payloads of a HEIF's `Exif` items and `mime`
/// items (XMP), found through `iinf` (the item types) and `iloc` (where
/// each item's bytes are).
///
/// Only the simple case is handled: an item with construction method 0
/// (its extents are offsets in this file) and data reference 0. An item
/// stored any other way (in `idat`, built from another item, or in another
/// file), or an extent that runs to the end of the file or past it, is
/// left as it is; nothing else in the file is touched. `None` when the
/// boxes do not hold together.
pub(super) fn wipe_heif(bytes: &mut [u8]) -> Option<()> {
    let (_, inner) = heif_meta(bytes)?;
    let targets = metadata_items(bytes, &inner)?;
    if targets.is_empty() {
        return Some(());
    }
    let Some(iloc) = inner.iter().find(|b| &b.kind == b"iloc") else {
        return Some(());
    };
    let mut wipe = Vec::new();
    let version = *bytes.get(iloc.payload)?;
    let mut at = iloc.payload + 4;
    let sizes = be16(bytes, at)?;
    at += 2;
    let offset_size = usize::from(sizes >> 12);
    let length_size = usize::from((sizes >> 8) & 0x0F);
    let base_offset_size = usize::from((sizes >> 4) & 0x0F);
    let index_size = if version == 0 {
        0
    } else {
        usize::from(sizes & 0x0F)
    };
    let (count, wide) = if version < 2 {
        (u32::from(be16(bytes, at)?), 2)
    } else {
        (be32(bytes, at)?, 4)
    };
    at += wide;
    for _ in 0..count {
        let item = if wide == 2 {
            u32::from(be16(bytes, at)?)
        } else {
            be32(bytes, at)?
        };
        at += wide;
        let method = if version == 0 {
            0
        } else {
            let m = be16(bytes, at)? & 0x0F;
            at += 2;
            m
        };
        let data_ref = be16(bytes, at)?;
        at += 2;
        let base = be_sized(bytes, at, base_offset_size)?;
        at += base_offset_size;
        let extents = be16(bytes, at)?;
        at += 2;
        for _ in 0..extents {
            at += index_size;
            let offset = be_sized(bytes, at, offset_size)?;
            at += offset_size;
            let length = be_sized(bytes, at, length_size)?;
            at += length_size;
            if method == 0 && data_ref == 0 && length > 0 && targets.contains(&item) {
                wipe.push((base.checked_add(offset)?, length));
            }
        }
    }
    for (offset, length) in wipe {
        let (Ok(start), Ok(length)) = (usize::try_from(offset), usize::try_from(length)) else {
            continue;
        };
        if let Some(range) = start
            .checked_add(length)
            .and_then(|end| bytes.get_mut(start..end))
        {
            range.fill(0);
        }
    }
    Some(())
}

/// The ids of the `Exif` and `mime` items an `iinf` lists.
fn metadata_items(bytes: &[u8], meta: &[BoxAt]) -> Option<Vec<u32>> {
    let Some(iinf) = meta.iter().find(|b| &b.kind == b"iinf") else {
        return Some(Vec::new());
    };
    let version = *bytes.get(iinf.payload)?;
    let first = iinf.payload + 4 + if version == 0 { 2 } else { 4 };
    let mut out = Vec::new();
    for infe in children(bytes, first, iinf.end)? {
        if &infe.kind != b"infe" {
            continue;
        }
        // Versions 0 and 1 carry no item type.
        let (id, kind_at) = match *bytes.get(infe.payload)? {
            2 => (u32::from(be16(bytes, infe.payload + 4)?), infe.payload + 8),
            3 => (be32(bytes, infe.payload + 4)?, infe.payload + 10),
            _ => continue,
        };
        if matches!(bytes.get(kind_at..kind_at + 4), Some(b"Exif" | b"mime")) {
            out.push(id);
        }
    }
    Some(out)
}
