//! Metadata removed without touching a pixel (specs/media.md 3.2 and 3.4).
//! Each walker copies the blocks it keeps verbatim and returns `None` when
//! the file's structure does not hold together.

use super::bytes::{be16, be32, le16, le32};

/// A JPEG without its metadata: every APPn segment goes but the ICC profile
/// (APP2 `ICC_PROFILE`) and Adobe's APP14 (which says how the colour
/// channels are coded, so dropping it would change the pixels), and so do
/// comments (COM). When the EXIF said the picture is turned, a minimal APP1
/// EXIF holding only that orientation goes back in after SOI. Everything
/// after EOI (MPF's extra pictures, vendor trailers) goes.
pub(super) fn jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    let mut orientation = None;
    let mut pos = 2;
    loop {
        if *bytes.get(pos)? != 0xFF {
            return None;
        }
        while *bytes.get(pos + 1)? == 0xFF {
            pos += 1;
        }
        let marker = *bytes.get(pos + 1)?;
        if marker == 0xD9 {
            out.extend_from_slice(&[0xFF, 0xD9]);
            break;
        }
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            out.extend_from_slice(&[0xFF, marker]);
            pos += 2;
            continue;
        }
        let len = usize::from(be16(bytes, pos + 2)?);
        if len < 2 {
            return None;
        }
        let end = pos + 2 + len;
        let segment = bytes.get(pos..end)?;
        let body = &segment[4..];
        let keep = match marker {
            0xE1 => {
                if body.starts_with(b"Exif\0\0") {
                    orientation = orientation.or_else(|| exif_orientation(&body[6..]));
                }
                false
            }
            0xE2 => body.starts_with(b"ICC_PROFILE\0"),
            0xEE => body.starts_with(b"Adobe"),
            0xE0..=0xEF | 0xFE => false,
            _ => true,
        };
        if keep {
            out.extend_from_slice(segment);
        }
        pos = end;
        if marker == 0xDA {
            // Entropy-coded data runs to the next marker that is not a
            // stuffed byte (FF00) or a restart (FFD0-FFD7). A file cut off
            // in its scan keeps what it has.
            let start = pos;
            loop {
                let (Some(&byte), Some(&next)) = (bytes.get(pos), bytes.get(pos + 1)) else {
                    out.extend_from_slice(bytes.get(start..).unwrap_or(&[]));
                    return Some(with_orientation(out, orientation));
                };
                if byte == 0xFF {
                    if next != 0x00 && !(0xD0..=0xD7).contains(&next) && next != 0xFF {
                        break;
                    }
                    pos += if next == 0xFF { 1 } else { 2 };
                } else {
                    pos += 1;
                }
            }
            out.extend_from_slice(&bytes[start..pos]);
        }
    }
    Some(with_orientation(out, orientation))
}

/// A stripped JPEG with the minimal EXIF after its SOI when the picture is
/// turned.
fn with_orientation(mut out: Vec<u8>, orientation: Option<u16>) -> Vec<u8> {
    if let Some(o) = orientation.filter(|&o| (2..=8).contains(&o)) {
        out.splice(2..2, orientation_exif(o));
    }
    out
}

/// The orientation (tag 0x0112) in IFD0 of a TIFF structure, as EXIF
/// carries it.
pub(super) fn exif_orientation(tiff: &[u8]) -> Option<u16> {
    let little = match tiff.get(0..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_at = |at| {
        if little {
            le16(tiff, at)
        } else {
            be16(tiff, at)
        }
    };
    let u32_at = |at| {
        if little {
            le32(tiff, at)
        } else {
            be32(tiff, at)
        }
    };
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let count = usize::from(u16_at(ifd)?);
    (0..count).find_map(|i| {
        let entry = ifd + 2 + i * 12;
        (u16_at(entry)? == 0x0112 && u16_at(entry + 2)? == 3).then(|| u16_at(entry + 8))?
    })
}

/// An APP1 segment of EXIF holding the orientation and nothing else.
fn orientation_exif(orientation: u16) -> Vec<u8> {
    let mut tiff = Vec::with_capacity(26);
    tiff.extend_from_slice(b"MM\0*");
    tiff.extend_from_slice(&8u32.to_be_bytes()); // IFD0 follows the header
    tiff.extend_from_slice(&1u16.to_be_bytes()); // one entry
    tiff.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
    tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_be_bytes()); // one of them
    tiff.extend_from_slice(&orientation.to_be_bytes());
    tiff.extend_from_slice(&[0, 0]); // the value's padding
    tiff.extend_from_slice(&0u32.to_be_bytes()); // no IFD1
    let len = 2 + 6 + tiff.len();
    let mut out = Vec::with_capacity(2 + len);
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&u16::try_from(len).unwrap_or(0).to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&tiff);
    out
}

/// A PNG without `eXIf`, `tEXt`, `iTXt`, `zTXt` and `tIME`, and without
/// anything after `IEND`.
pub(super) fn png(bytes: &[u8]) -> Option<Vec<u8>> {
    let signature = bytes.get(..8)?;
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(signature);
    let mut pos = 8;
    loop {
        let len = usize::try_from(be32(bytes, pos)?).ok()?;
        let end = pos.checked_add(12)?.checked_add(len)?;
        let chunk = bytes.get(pos..end)?;
        let kind = &chunk[4..8];
        if !matches!(kind, b"eXIf" | b"tEXt" | b"iTXt" | b"zTXt" | b"tIME") {
            out.extend_from_slice(chunk);
        }
        if kind == b"IEND" {
            return Some(out);
        }
        pos = end;
    }
}

/// A WebP without its `EXIF` and `XMP ` chunks, their flags cleared in
/// `VP8X`, and the RIFF size made to match.
pub(super) fn webp(bytes: &[u8]) -> Option<Vec<u8>> {
    let riff_end = usize::try_from(le32(bytes, 4)?).ok()?.checked_add(8)?;
    let end = riff_end.min(bytes.len());
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(bytes.get(..12)?);
    let mut pos = 12;
    while pos + 8 <= end {
        let len = usize::try_from(le32(bytes, pos + 4)?).ok()?;
        let padded = len + (len & 1);
        let chunk_end = (pos + 8).checked_add(padded)?.min(end);
        let chunk = bytes.get(pos..chunk_end)?;
        match &chunk[..4] {
            b"EXIF" | b"XMP " => {}
            b"VP8X" => {
                let at = out.len() + 8;
                out.extend_from_slice(chunk);
                if let Some(flags) = out.get_mut(at) {
                    *flags &= !(0x08 | 0x04);
                }
            }
            _ => out.extend_from_slice(chunk),
        }
        pos = chunk_end;
    }
    let size = u32::try_from(out.len() - 8).ok()?;
    out.get_mut(4..8)?.copy_from_slice(&size.to_le_bytes());
    Some(out)
}

/// A GIF without comment extensions and without application extensions
/// other than `NETSCAPE2.0`, and without anything after the trailer; with
/// the number of frames it holds.
pub(super) fn gif(bytes: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut out = Vec::with_capacity(bytes.len());
    let packed = *bytes.get(10)?;
    let mut pos = 13 + color_table(packed);
    out.extend_from_slice(bytes.get(..pos)?);
    let mut frames = 0;
    loop {
        match *bytes.get(pos)? {
            0x3B => {
                out.push(0x3B);
                return Some((out, frames));
            }
            0x2C => {
                let packed = *bytes.get(pos + 9)?;
                // Descriptor, local table, LZW code size, then the data.
                let data = pos + 10 + color_table(packed) + 1;
                let end = sub_blocks(bytes, data)?;
                out.extend_from_slice(bytes.get(pos..end)?);
                frames += 1;
                pos = end;
            }
            0x21 => {
                let label = *bytes.get(pos + 1)?;
                let end = sub_blocks(bytes, pos + 2)?;
                let keep = match label {
                    0xFE => false,
                    0xFF => {
                        bytes.get(pos + 2) == Some(&11)
                            && bytes.get(pos + 3..pos + 14) == Some(b"NETSCAPE2.0")
                    }
                    _ => true,
                };
                if keep {
                    out.extend_from_slice(bytes.get(pos..end)?);
                }
                pos = end;
            }
            _ => return None,
        }
    }
}

/// The length of the colour table a GIF packed field announces.
fn color_table(packed: u8) -> usize {
    if packed & 0x80 == 0 {
        0
    } else {
        3 << ((packed & 0x07) + 1)
    }
}

/// The end of a run of GIF sub-blocks starting at `pos` (past its
/// terminator).
fn sub_blocks(bytes: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let len = usize::from(*bytes.get(pos)?);
        pos += 1 + len;
        if len == 0 {
            return Some(pos);
        }
    }
}
