//! Bounds-checked integer reads for the container walkers. Every read of
//! a file's bytes goes through these, so a short or lying file is `None`
//! and never a panic.

pub(super) fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

pub(super) fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

pub(super) fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        b.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}

pub(super) fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

pub(super) fn le24(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(3)?)?;
    Some(u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16)
}

pub(super) fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// A big-endian unsigned integer of 0, 4 or 8 bytes (ISO BMFF's variable
/// fields); 0 bytes reads as 0.
pub(super) fn be_sized(b: &[u8], at: usize, size: usize) -> Option<u64> {
    match size {
        0 => Some(0),
        4 => be32(b, at).map(u64::from),
        8 => be64(b, at),
        _ => None,
    }
}
