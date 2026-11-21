//! Image work for the instance (architecture 3 and 12), as a NIF on a dirty CPU scheduler.
//!
//! Everything here reads bytes someone uploaded. The rules, in the order they are applied:
//! the format is one of four, told by its first bytes and never by what the upload called
//! itself; the dimensions are read from the header and refused past the bounds before a
//! pixel is decoded; the decoder runs under an allocation limit; and what goes back out is
//! encoded here from the pixels, so nothing of the upload's own bytes (its metadata, where
//! a camera puts where and when) is ever stored or served. A panic is caught by Rustler
//! and raised to the caller, which treats it as a refusal.

use std::io::Cursor;

use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngDecoder;
use image::error::{ImageError, ImageResult, LimitError, LimitErrorKind};
use image::imageops::FilterType;
use image::{
    metadata::Orientation, AnimationDecoder, ColorType, Delay, DynamicImage, Frame, Frames,
    ImageDecoder, ImageFormat, ImageReader, Limits, RgbImage, RgbaImage,
};
use moxcms::{
    ColorProfile, DataColorSpace, Layout, ParsingOptions, TransformExecutor, TransformOptions,
};
use rustler::{Atom, Binary, Encoder, Env, OwnedBinary, Term};

mod atoms {
    rustler::atoms! {
        ok,
        error,
        /// Not one of the formats taken, or not an image at all.
        unsupported,
        /// Wider, taller or larger than the bounds.
        too_large,
        /// One of the formats, and it would not decode.
        invalid,
    }
}

/// The longest side an upload may declare, and the most pixels.
const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: u64 = 25_000_000;
/// What the decoder may allocate: the pixels above as RGBA, and room to work. Two of
/// these at once (`Lumia.Media`) have to fit beside the instance on its smallest machine.
const MAX_ALLOC: u64 = 160 * 1024 * 1024;
/// The side of the square an avatar is kept as.
const AVATAR_SIDE: u32 = 512;
/// The longest side a link card's picture is kept at (specs/media.md 6.3): twice the
/// widest a client draws one, for a display of two pixels a point.
const CARD_SIDE: u32 = 800;
const JPEG_QUALITY: u8 = 85;
/// The longest side a custom emoji is kept at (specs/emoji.md 3).
const EMOJI_SIDE: u32 = 128;
/// An animated upload's canvas, its frames, and the pixels decoded across them.
const ANIMATED_SIDE: u32 = 1024;
const ANIMATED_FRAMES: usize = 200;
const ANIMATED_PIXELS: u64 = 64_000_000;
/// The most an animated emoji may come to once encoded here.
const ANIMATED_MAX_BYTES: usize = 512 * 1024;
/// A frame's delay under this is drawn by browsers as 100 ms, and kept so.
const MIN_DELAY_MS: u32 = 20;
const DEFAULT_DELAY_MS: u32 = 100;

enum Refusal {
    Unsupported,
    TooLarge,
    Invalid,
}

impl Refusal {
    fn atom(&self) -> Atom {
        match self {
            Self::Unsupported => atoms::unsupported(),
            Self::TooLarge => atoms::too_large(),
            Self::Invalid => atoms::invalid(),
        }
    }
}

/// A reader over an upload, and the format its first bytes say it is.
type Probed<'a> = (ImageReader<Cursor<&'a [u8]>>, ImageFormat);

fn reader(bytes: &[u8]) -> Result<Probed<'_>, Refusal> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Refusal::Unsupported)?;
    match reader.format() {
        Some(
            format @ (ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif),
        ) => Ok((reader, format)),
        _ => Err(Refusal::Unsupported),
    }
}

/// The chunks of a RIFF body from `at` on, each its code and its payload. A chunk that says
/// it runs past the bytes is `Invalid`; fewer than a header's eight bytes left is the end.
struct RiffChunks<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Iterator for RiffChunks<'a> {
    type Item = Result<([u8; 4], &'a [u8]), Refusal>;

    fn next(&mut self) -> Option<Self::Item> {
        let header = self.bytes.get(self.at..self.at.checked_add(8)?)?;
        let fourcc = [header[0], header[1], header[2], header[3]];
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let start = self.at + 8;
        let Some(payload) = start
            .checked_add(size)
            .and_then(|end| self.bytes.get(start..end))
        else {
            self.at = self.bytes.len();
            return Some(Err(Refusal::Invalid));
        };
        // Padded to an even length; past the end, the next call ends the walk.
        self.at = start + size + (size & 1);
        Some(Ok((fourcc, payload)))
    }
}

fn le24(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16
}

/// What a WebP's chunks say, read before any decoder sees them: how many frames it has, and
/// what image-webp (0.2.4) will allocate to decode it beyond the pixels it hands back.
struct WebpCost {
    /// The `ANMF` chunks of an animated one, or 0 for a still one.
    frames: usize,
    /// The most held at once beside the output buffer by `read_image`: a still picture, or an
    /// animation's first frame.
    still: u64,
    /// The most held at once beside the frame's own buffer by any one `read_frame`, the
    /// canvas it composes on included.
    frame: u64,
}

/// What image-webp holds whatever the picture: its chunk map, the readers, and the vectors
/// of a few kilobytes that follow the picture's width (a row of borders, a row of macroblocks).
const WEBP_FIXED: u64 = 256 * 1024;

/// How an alpha chunk is stored, from its first byte, which decides what reading it takes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AlphaStorage {
    /// No `ALPH` chunk, or one the decoder refuses before it allocates for it.
    None,
    /// Raw, a byte a pixel.
    Raw,
    /// A lossless bitstream's green channel.
    Lossless,
}

fn alpha_storage(payload: &[u8]) -> AlphaStorage {
    match payload.first().map(|b| b & 0x03) {
        Some(0) => AlphaStorage::Raw,
        Some(1) => AlphaStorage::Lossless,
        _ => AlphaStorage::None,
    }
}

fn pixels(width: u32, height: u32) -> u64 {
    u64::from(width) * u64::from(height)
}

/// A lossy bitstream's planes (`Vp8Decoder::read_frame_header`): Y, U and V of whole 16-pixel
/// macroblocks, 384 bytes each, kept in the `Frame` it returns.
fn vp8_planes((width, height): (u32, u32)) -> u64 {
    pixels(width.div_ceil(16), height.div_ceil(16)) * 384
}

/// The most `Vp8Decoder::decode_frame` holds for a bitstream of `size` and `len` bytes: its
/// planes; every macroblock's header, 30 bytes each in a vector grown by doubling, so a
/// half-full one and the one it grows into at once; and its partitions, copied from the
/// bitstream into buffers of their own (the last read whole, then copied again).
fn vp8_peak(size: (u32, u32), len: usize) -> u64 {
    let macroblocks = pixels(size.0.div_ceil(16), size.1.div_ceil(16)).next_power_of_two();
    vp8_planes(size) + macroblocks * 3 / 2 * 30 + 4 * len as u64
}

/// The most a lossless bitstream of `width` by `height` holds beside the pixels it decodes
/// into (`LosslessDecoder`): a predictor and a colour transform's data and its entropy image,
/// each at most four bytes a 4 by 4 block, the entropy image again as a `u16` a block while
/// it is read, and two colour caches of 8 KiB.
///
/// Its prefix-code tables are not counted here: how many there are is told by the entropy
/// image's values, not the picture's size, and a stream can be made with tens of thousands.
/// image-webp counts them itself, as it reads them, against what the decode has left once
/// this is reserved ([`Webp`]); an ordinary stream's are small (a photograph of 4,000 a side
/// from cwebp: under 1 MB).
fn lossless_working((width, height): (u32, u32)) -> u64 {
    pixels(width.div_ceil(4), height.div_ceil(4)) * 14 + 2 * 8192
}

/// The most `read_alpha_chunk` holds for an alpha plane of `size`: raw, a byte a pixel; lossless,
/// the plane decoded as RGBA, its green channel copied out of it, and the decoder's working.
fn alpha_peak(size: (u32, u32), storage: AlphaStorage) -> u64 {
    let n = pixels(size.0, size.1);
    match storage {
        AlphaStorage::None => 0,
        AlphaStorage::Raw => n,
        AlphaStorage::Lossless => 5 * n + lossless_working(size),
    }
}

/// An `ANMF` chunk's frame as `read_frame` decodes it on a canvas of `canvas` pixels: the
/// frame's own buffer (RGB for a lossy frame, RGBA otherwise) and what decoding it takes, then
/// the canvas, made at the first frame and kept. `None` is a frame the decoder refuses before
/// it allocates for it.
fn frame_peak(payload: &[u8], canvas: u64) -> Result<Option<u64>, Refusal> {
    let Some(header) = payload.get(..16) else {
        return Ok(None);
    };
    let size = (le24(&header[6..9]) + 1, le24(&header[9..12]) + 1);
    let n = pixels(size.0, size.1);
    let mut subs = RiffChunks {
        bytes: payload.get(16..).unwrap_or_default(),
        at: 0,
    };
    let decoding = match subs.next().transpose()? {
        Some((fourcc, bitstream)) if &fourcc == b"VP8 " => vp8_size(bitstream)
            .map(|lossy| vp8_peak(lossy, bitstream.len()).max(vp8_planes(lossy) + 3 * n)),
        Some((fourcc, _)) if &fourcc == b"VP8L" => Some(4 * n + lossless_working(size)),
        Some((fourcc, alph)) if &fourcc == b"ALPH" => {
            let storage = alpha_storage(alph);
            // The alpha plane is read first and its last copy kept, a byte a pixel, while the
            // lossy bitstream after it is decoded and drawn into the RGBA frame.
            let kept = if storage == AlphaStorage::None { 0 } else { n };
            let lossy = match subs.next().transpose()? {
                Some((fourcc, bitstream)) if &fourcc == b"VP8 " => vp8_size(bitstream)
                    .map(|lossy| vp8_peak(lossy, bitstream.len()).max(vp8_planes(lossy) + 4 * n)),
                _ => None,
            };
            Some(alpha_peak(size, storage).max(lossy.map_or(0, |l| kept + l)))
        }
        _ => None,
    };
    Ok(decoding.map(|d| d + 4 * canvas))
}

/// What a WebP's chunks say, read before any decoder sees them: its frames, and what decoding
/// it costs ([`WebpCost`]).
///
/// image-webp (0.2.4) allocates for a lossy bitstream by the size the bitstream's own header
/// gives, some 400 MB for 16,383 a side, and only then compares that with the canvas or frame
/// it is in (`Vp8Decoder::read_frame_header`). So the header of every `VP8 ` chunk, at the top
/// or in a frame, is read here and refused when it says more than the canvas, which the
/// bounds have been or will be checked against. It also allocates a metadata chunk by the
/// size the chunk claims, so a chunk claiming more than the upload holds is refused here too.
///
/// And it allocates beside the pixels it returns, by path (`WebPDecoder::read_image`,
/// `read_frame`, `read_alpha_chunk`): a lossy picture keeps its planes, about a byte and a half
/// a pixel; one with an alpha chunk then reads that chunk, a lossless one as five bytes a pixel
/// more; a lossless picture without alpha is decoded as RGBA into a buffer of its own, four
/// bytes a pixel, then copied out as RGB; an animation's frame is decoded into a buffer of its
/// own and composed on a canvas of four bytes a pixel. Each is worked out here from the chunks
/// the decoder will read, the largest it could take where a file gives it a choice.
fn webp_cost(bytes: &[u8]) -> Result<WebpCost, Refusal> {
    if bytes.get(..4) != Some(b"RIFF".as_slice()) || bytes.get(8..12) != Some(b"WEBP".as_slice()) {
        return Err(Refusal::Invalid);
    }
    let mut chunks = RiffChunks { bytes, at: 12 };
    // A simple WebP is its one bitstream, and the size the decoder reports, which the bounds
    // are checked against, is the one in that bitstream's header.
    let header = match chunks.next() {
        Some(Ok((fourcc, payload))) if &fourcc == b"VP8X" => {
            payload.get(..10).ok_or(Refusal::Invalid)?
        }
        Some(Ok((fourcc, payload))) if &fourcc == b"VP8 " => {
            let still = vp8_size(payload).map_or(0, |size| vp8_peak(size, payload.len()));
            return Ok(WebpCost {
                frames: 0,
                still: WEBP_FIXED + still,
                frame: 0,
            });
        }
        Some(Ok((fourcc, payload))) if &fourcc == b"VP8L" => {
            let still = vp8l_header(payload).map_or(0, |(size, alpha)| {
                let rgba = if alpha { 0 } else { 4 * pixels(size.0, size.1) };
                rgba + lossless_working(size)
            });
            return Ok(WebpCost {
                frames: 0,
                still: WEBP_FIXED + still,
                frame: 0,
            });
        }
        // Not a WebP the decoder reads; it refuses it before allocating anything.
        Some(Ok(_)) => {
            return Ok(WebpCost {
                frames: 0,
                still: WEBP_FIXED,
                frame: 0,
            })
        }
        _ => return Err(Refusal::Invalid),
    };
    let canvas = (le24(&header[4..7]) + 1, le24(&header[7..10]) + 1);
    let n = pixels(canvas.0, canvas.1);
    let alpha = header[0] & 0x10 != 0;
    let animated = header[0] & 0x02 != 0;
    let mut frames = 0;
    // A still picture's image chunks are the first of each kind, at the top or failing that
    // in the first frame; any of them is counted, so whichever the decoder takes is.
    let mut lossy: Option<((u32, u32), usize)> = None;
    let mut lossless = false;
    let mut storage = AlphaStorage::None;
    let mut first_frame = None;
    let mut frame = 0u64;
    let mut seen = |fourcc: &[u8; 4], payload: &[u8]| -> Result<(), Refusal> {
        match fourcc {
            b"VP8 " => {
                vp8_within(payload, canvas)?;
                if let Some(size) = vp8_size(payload) {
                    let (old, len) = lossy.unwrap_or(((0, 0), 0));
                    lossy = Some((
                        (old.0.max(size.0), old.1.max(size.1)),
                        len.max(payload.len()),
                    ));
                }
            }
            b"VP8L" => lossless = true,
            b"ALPH" => storage = storage.max(alpha_storage(payload)),
            _ => {}
        }
        Ok(())
    };
    // To the end of the bytes, not of the RIFF: the decoder reads as far.
    for chunk in chunks {
        let (fourcc, payload) = chunk?;
        if &fourcc == b"ANMF" {
            frames += 1;
            // The decoder goes from one frame to the next by the unpadded size, so an
            // odd one would put a frame where this walk does not look.
            if payload.len() % 2 == 1 {
                return Err(Refusal::Invalid);
            }
            let inner = payload.get(16..).ok_or(Refusal::Invalid)?;
            for sub in (RiffChunks {
                bytes: inner,
                at: 0,
            }) {
                let (fourcc, payload) = sub?;
                seen(&fourcc, payload)?;
            }
            let peak = frame_peak(payload, n)?.unwrap_or(0);
            first_frame.get_or_insert(peak);
            frame = frame.max(peak);
        } else {
            seen(&fourcc, payload)?;
        }
    }
    let still = if animated {
        first_frame.unwrap_or(0)
    } else {
        // Decoded as RGBA into the output when it has alpha, else into a buffer of its own.
        let lossless = if lossless {
            (if alpha { 0 } else { 4 * n }) + lossless_working(canvas)
        } else {
            0
        };
        // Its planes kept while an alpha plane of the canvas's size is read.
        let lossy = lossy.map_or(0, |(size, len)| {
            let alpha = if alpha {
                alpha_peak(canvas, storage)
            } else {
                0
            };
            vp8_peak(size, len).max(vp8_planes(size) + alpha)
        });
        lossless.max(lossy)
    };
    Ok(WebpCost {
        frames: if animated { frames } else { 0 },
        still: WEBP_FIXED + still,
        frame: WEBP_FIXED + frame,
    })
}

/// A lossy bitstream's size, from its keyframe header; `None` for one too short to have that
/// header or not a keyframe, which the decoder refuses before it allocates for its size.
fn vp8_size(payload: &[u8]) -> Option<(u32, u32)> {
    let h = payload.get(..10)?;
    if h[0] & 1 != 0 {
        return None;
    }
    let w = u32::from(u16::from_le_bytes([h[6], h[7]]) & 0x3FFF);
    let hh = u32::from(u16::from_le_bytes([h[8], h[9]]) & 0x3FFF);
    Some((w, hh))
}

/// A lossy bitstream's size refused past `(width, height)`.
fn vp8_within(payload: &[u8], (width, height): (u32, u32)) -> Result<(), Refusal> {
    match vp8_size(payload) {
        Some((w, h)) if w > width || h > height => Err(Refusal::TooLarge),
        _ => Ok(()),
    }
}

/// A simple lossless WebP's size and whether it says it has alpha, read as image-webp reads
/// them (`WebPDecoder::read_data`), carry and all.
fn vp8l_header(payload: &[u8]) -> Option<((u32, u32), bool)> {
    let h = payload.get(..5)?;
    if h[0] != 0x2f {
        return None;
    }
    let bits = u32::from_le_bytes([h[1], h[2], h[3], h[4]]);
    let width = 1u32.wrapping_add(bits) & 0x3FFF;
    let height = (1 + (bits >> 14)) & 0x3FFF;
    Some(((width, height), (bits >> 28) & 1 != 0))
}

/// image-webp's decoder as `image` reads it, but with the allocation limit left over handed on
/// to image-webp's own memory limit, which `image`'s wrapper does not do. That limit covers
/// what the chunk walk cannot know before decoding: a lossless stream's prefix codes, whose
/// number its entropy image sets (patched in; vendor/image-webp/PATCHES.md).
struct Webp<'a>(image_webp::WebPDecoder<Cursor<&'a [u8]>>);

impl<'a> Webp<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, Refusal> {
        image_webp::WebPDecoder::new(Cursor::new(bytes))
            .map(Self)
            .map_err(|_| Refusal::Invalid)
    }
}

/// An image-webp error as `image` has it, its memory limit as a limit.
fn webp_error(error: image_webp::DecodingError) -> ImageError {
    match error {
        image_webp::DecodingError::MemoryLimitExceeded => {
            ImageError::Limits(LimitError::from_kind(LimitErrorKind::InsufficientMemory))
        }
        error => ImageError::Decoding(image::error::DecodingError::new(
            ImageFormat::WebP.into(),
            error,
        )),
    }
}

/// A decoder's error as a refusal: past a limit is too large, anything else will not decode.
fn refusal(error: ImageError) -> Refusal {
    match error {
        ImageError::Limits(_) => Refusal::TooLarge,
        _ => Refusal::Invalid,
    }
}

impl ImageDecoder for Webp<'_> {
    fn dimensions(&self) -> (u32, u32) {
        self.0.dimensions()
    }

    fn color_type(&self) -> ColorType {
        if self.0.has_alpha() {
            ColorType::Rgba8
        } else {
            ColorType::Rgb8
        }
    }

    fn icc_profile(&mut self) -> ImageResult<Option<Vec<u8>>> {
        self.0.icc_profile().map_err(webp_error)
    }

    fn exif_metadata(&mut self) -> ImageResult<Option<Vec<u8>>> {
        self.0.exif_metadata().map_err(webp_error)
    }

    fn set_limits(&mut self, limits: Limits) -> ImageResult<()> {
        let (width, height) = self.dimensions();
        limits.check_dimensions(width, height)?;
        let left = limits.max_alloc.map_or(usize::MAX, |left| {
            usize::try_from(left).unwrap_or(usize::MAX)
        });
        self.0.set_memory_limit(left);
        Ok(())
    }

    fn read_image(mut self, buf: &mut [u8]) -> ImageResult<()> {
        self.0.read_image(buf).map_err(webp_error)
    }

    fn read_image_boxed(self: Box<Self>, buf: &mut [u8]) -> ImageResult<()> {
        (*self).read_image(buf)
    }
}

impl<'a> AnimationDecoder<'a> for Webp<'a> {
    /// Each frame as `image`'s wrapper reads them: in a buffer of its own, RGBA, or RGB and
    /// then a copy of it as RGBA.
    fn into_frames(mut self) -> Frames<'a> {
        let (width, height) = self.0.dimensions();
        let alpha = self.0.has_alpha();
        let mut left = self.0.num_frames();
        Frames::new(Box::new(std::iter::from_fn(move || {
            left = left.checked_sub(1)?;
            let read = if alpha {
                let mut pixels = RgbaImage::new(width, height);
                self.0.read_frame(&mut pixels).map(|delay| (pixels, delay))
            } else {
                let mut pixels = RgbImage::new(width, height);
                self.0
                    .read_frame(&mut pixels)
                    .map(|delay| (DynamicImage::ImageRgb8(pixels).to_rgba8(), delay))
            };
            match read {
                Err(image_webp::DecodingError::NoMoreFrames) => None,
                read => Some(read.map_err(webp_error).map(|(pixels, delay)| {
                    Frame::from_parts(pixels, 0, 0, Delay::from_numer_denom_ms(delay, 1))
                })),
            }
        })))
    }
}

/// The frames a PNG's `acTL` chunk says it has, read before its image data; 0 for a PNG
/// without one.
fn apng_frames(bytes: &[u8]) -> u32 {
    let mut at = 8usize;
    while let Some(header) = at.checked_add(8).and_then(|end| bytes.get(at..end)) {
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        match &header[4..8] {
            b"acTL" => {
                return bytes
                    .get(at + 8..at + 12)
                    .map_or(0, |n| u32::from_be_bytes([n[0], n[1], n[2], n[3]]));
            }
            b"IDAT" => return 0,
            _ => {}
        }
        // Length, type, data and checksum.
        match at.checked_add(12).and_then(|a| a.checked_add(len)) {
            Some(next) => at = next,
            None => return 0,
        }
    }
    0
}

/// How many images a GIF holds, counted to two, by walking its blocks without decoding any.
fn gif_images(bytes: &[u8]) -> usize {
    // A colour table's bytes, from the flags of the block it follows.
    let table = |flags: u8| {
        if flags & 0x80 != 0 {
            3usize << ((flags & 7) + 1)
        } else {
            0
        }
    };
    // Past a run of sub-blocks, each its length and its bytes, ended by a zero.
    let blocks = |mut at: usize| -> Option<usize> {
        loop {
            let len = usize::from(*bytes.get(at)?);
            at += 1 + len;
            if len == 0 {
                return Some(at);
            }
        }
    };
    let Some(&flags) = bytes.get(10) else {
        return 0;
    };
    let mut at = 13 + table(flags);
    let mut images = 0;
    while images < 2 {
        let next = match bytes.get(at) {
            // An extension: its label, then its sub-blocks.
            Some(0x21) => blocks(at + 2),
            // An image: its descriptor, its own colour table, the LZW code size, its data.
            Some(0x2C) => {
                images += 1;
                bytes
                    .get(at + 9)
                    .and_then(|&flags| blocks(at + 10 + table(flags) + 1))
            }
            _ => None,
        };
        match next {
            Some(next) => at = next,
            None => break,
        }
    }
    images
}

/// The upload as pixels, upright and in sRGB: bounds first, then the decode under its
/// limits, then its colour profile applied and the turn its metadata asks for (a phone's
/// photo is stored on its side and says so, in Display P3 and says so).
fn decode(bytes: &[u8]) -> Result<DynamicImage, Refusal> {
    decode_within(bytes, MAX_ALLOC).map(|(image, _)| image)
}

/// [`decode`] under an allocation limit of `max_alloc` bytes, and the bytes it reserved
/// against that limit before decoding: the most it holds at once.
fn decode_within(bytes: &[u8], max_alloc: u64) -> Result<(DynamicImage, u64), Refusal> {
    let (probe, format) = reader(bytes)?;
    // What the decoder works in beside the pixels, where that is known from the bytes.
    let working = if format == ImageFormat::WebP {
        webp_cost(bytes)?.still
    } else {
        0
    };
    let (width, height) = probe.into_dimensions().map_err(|_| Refusal::Invalid)?;
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(Refusal::TooLarge);
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(max_alloc);
    let mut decoder: Box<dyn ImageDecoder> = if format == ImageFormat::WebP {
        Box::new(Webp::new(bytes)?)
    } else {
        let (mut reader, _) = reader(bytes)?;
        reader.limits(limits.clone());
        Box::new(reader.into_decoder().map_err(|_| Refusal::Invalid)?)
    };
    let orientation = decoder.orientation().map_err(|_| Refusal::Invalid)?;
    // A profile that will not read is taken for none: the picture is sRGB, as most are.
    let icc = decoder.icc_profile().ok().flatten();
    // Everything held at once counts against the limit before any of it is allocated: the
    // decoded pixels, as `ImageReader::decode` counts them (a picture within the bounds can
    // still need more, at 16 bits a channel, and `from_decoder` does not ask); beside them
    // what the decoder works in, or the copy a quarter turn makes once it is done; and the
    // profile, kept through both.
    let pixels = decoder.total_bytes();
    let turned = match orientation {
        Orientation::Rotate90
        | Orientation::Rotate270
        | Orientation::Rotate90FlipH
        | Orientation::Rotate270FlipH => pixels,
        _ => 0,
    };
    let need = pixels
        .saturating_add(working.max(turned))
        .saturating_add(icc.as_ref().map_or(0, |icc| icc.len() as u64));
    limits.reserve(need).map_err(|_| Refusal::TooLarge)?;
    // What is left goes to the decoder, for what is not known before decoding (a lossless
    // WebP's prefix codes).
    decoder.set_limits(limits).map_err(|_| Refusal::TooLarge)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(refusal)?;
    if let Some(icc) = icc {
        to_srgb(&mut image, &icc);
    }
    image.apply_orientation(orientation);
    Ok((image, need))
}

/// The largest embedded colour profile read: a display's is a few hundred bytes, a
/// printer's with its lookup tables a few hundred kilobytes.
const MAX_ICC_BYTES: usize = 1 << 20;
/// The largest colour lookup table a profile may build, in entries.
const MAX_CLUT: usize = 1 << 20;

/// Converts, in place, pixels in the colour space an embedded ICC profile names (Display
/// P3 from a phone, Adobe RGB from a camera) to sRGB, which is what a picture without one
/// is taken to be; the profile itself is not kept. A profile that does not parse or is not
/// RGB, and pixels that are not RGB of 8 or 16 bits, are left as they are: a grey picture
/// is grey in any profile.
fn to_srgb(image: &mut DynamicImage, icc: &[u8]) {
    let options = ParsingOptions {
        max_profile_size: MAX_ICC_BYTES,
        max_allowed_clut_size: MAX_CLUT,
        ..ParsingOptions::default()
    };
    let Ok(profile) = ColorProfile::new_from_slice_with_options(icc, options) else {
        return;
    };
    if profile.color_space != DataColorSpace::Rgb {
        return;
    }
    let srgb = ColorProfile::new_srgb();
    let width = image.width() as usize;
    let to = |layout| {
        profile
            .create_transform_8bit(layout, &srgb, layout, TransformOptions::default())
            .ok()
    };
    let to16 = |layout| {
        profile
            .create_transform_16bit(layout, &srgb, layout, TransformOptions::default())
            .ok()
    };
    match image {
        DynamicImage::ImageRgb8(pixels) => {
            if let Some(t) = to(Layout::Rgb) {
                convert(t.as_ref(), pixels, width * 3);
            }
        }
        DynamicImage::ImageRgba8(pixels) => {
            if let Some(t) = to(Layout::Rgba) {
                convert(t.as_ref(), pixels, width * 4);
            }
        }
        DynamicImage::ImageRgb16(pixels) => {
            if let Some(t) = to16(Layout::Rgb) {
                convert(t.as_ref(), pixels, width * 3);
            }
        }
        DynamicImage::ImageRgba16(pixels) => {
            if let Some(t) = to16(Layout::Rgba) {
                convert(t.as_ref(), pixels, width * 4);
            }
        }
        _ => {}
    }
}

/// `samples` run through `transform` a row of `row` samples at a time, so the copy it
/// needs is one row and not the picture.
fn convert<V: Copy + Default>(transform: &dyn TransformExecutor<V>, samples: &mut [V], row: usize) {
    if row == 0 {
        return;
    }
    let mut scratch = vec![V::default(); row];
    for line in samples.chunks_exact_mut(row) {
        scratch.copy_from_slice(line);
        if transform.transform(&scratch, line).is_err() {
            return;
        }
    }
}

/// The middle square of `image`, no larger than [`AVATAR_SIDE`]; a smaller one is kept
/// at its size, not blown up.
fn square(image: &DynamicImage) -> DynamicImage {
    let side = image.width().min(image.height());
    let (x, y) = ((image.width() - side) / 2, (image.height() - side) / 2);
    let cropped = image.crop_imm(x, y, side, side);
    if side > AVATAR_SIDE {
        cropped.resize_exact(AVATAR_SIDE, AVATAR_SIDE, FilterType::CatmullRom)
    } else {
        cropped
    }
}

/// `image` with its longest side at most [`CARD_SIDE`] and its shape kept; a smaller one
/// is kept at its size.
fn fit(image: &DynamicImage) -> DynamicImage {
    if image.width().max(image.height()) > CARD_SIDE {
        image.resize(CARD_SIDE, CARD_SIDE, FilterType::CatmullRom)
    } else {
        image.clone()
    }
}

/// `image` within `side` by `side`, its shape kept; a smaller one is kept at its size.
fn fit_within(image: &DynamicImage, side: u32) -> DynamicImage {
    if image.width().max(image.height()) > side {
        image.resize(side, side, FilterType::CatmullRom)
    } else {
        image.clone()
    }
}

/// An uploaded picture as a custom emoji (specs/emoji.md 3): its media type, bytes and size,
/// and whether it is animated. Still pictures come out as PNG within [`EMOJI_SIDE`]; a GIF,
/// an APNG or a WebP of more than one frame comes out as an animated GIF, every frame fitted
/// the same.
fn emoji_picture(bytes: &[u8]) -> Result<(&'static str, Vec<u8>, u32, u32, bool), Refusal> {
    let (_, format) = reader(bytes)?;
    // Still or animated first, from the frames the file says it has, and only then the bounds
    // of the one it is: a single frame is a still picture whatever its format, held to a
    // still picture's bounds, and more than one are held to an animation's.
    let frames = match format {
        ImageFormat::Gif if gif_images(bytes) > 1 => Some(animated_frames(gif_decoder(bytes)?)?),
        ImageFormat::Png if apng_frames(bytes) > 1 => {
            // The allocation limit goes to the png crate only here, at construction
            // (`set_limits` checks dimensions alone); the canvas is checked by hand after
            // the header, so that one too large says so.
            let mut limits = animated_limits();
            limits.max_image_width = None;
            limits.max_image_height = None;
            let mut decoder = PngDecoder::with_limits(Cursor::new(bytes), limits)
                .map_err(|_| Refusal::Invalid)?;
            let canvas = check_canvas(decoder.dimensions())?;
            decoder
                .set_limits(animated_limits())
                .map_err(|_| Refusal::TooLarge)?;
            if decoder.is_apng().unwrap_or(false) {
                let apng = decoder.apng().map_err(|_| Refusal::Invalid)?;
                Some(collect_frames(apng.into_frames(), canvas)?)
            } else {
                None
            }
        }
        ImageFormat::WebP => match webp_cost(bytes)? {
            cost if cost.frames > 1 => {
                let mut decoder = Webp::new(bytes)?;
                check_canvas(decoder.dimensions())?;
                let mut limits = animated_limits();
                limits
                    .reserve(animated_need(&decoder, &cost))
                    .map_err(|_| Refusal::TooLarge)?;
                decoder.set_limits(limits).map_err(|_| Refusal::TooLarge)?;
                Some(animated_frames(decoder)?)
            }
            _ => None,
        },
        _ => None,
    };
    match frames {
        Some(frames) if frames.len() > 1 => {
            let fitted: Vec<Frame> = frames
                .into_iter()
                .map(|(mut pixels, delay)| {
                    // GIF has one transparent colour: a pixel under half opaque is see-through,
                    // any other is solid.
                    for p in pixels.pixels_mut() {
                        p.0[3] = if p.0[3] < 128 { 0 } else { 255 };
                    }
                    Frame::from_parts(pixels, 0, 0, Delay::from_numer_denom_ms(delay, 1))
                })
                .collect();
            let (width, height) = fitted
                .first()
                .map(|f| (f.buffer().width(), f.buffer().height()))
                .ok_or(Refusal::Invalid)?;
            let mut out = Vec::new();
            {
                let mut encoder = GifEncoder::new_with_speed(&mut out, 10);
                encoder
                    .set_repeat(Repeat::Infinite)
                    .map_err(|_| Refusal::Invalid)?;
                encoder
                    .encode_frames(fitted)
                    .map_err(|_| Refusal::Invalid)?;
            }
            if out.len() > ANIMATED_MAX_BYTES {
                return Err(Refusal::TooLarge);
            }
            Ok(("image/gif", out, width, height, true))
        }
        Some(mut one) => {
            let (buffer, _) = one.pop().ok_or(Refusal::Invalid)?;
            still_emoji(&DynamicImage::ImageRgba8(buffer))
        }
        None => still_emoji(&decode(bytes)?),
    }
}

fn still_emoji(image: &DynamicImage) -> Result<(&'static str, Vec<u8>, u32, u32, bool), Refusal> {
    let fitted = fit_within(image, EMOJI_SIDE);
    let mut out = Vec::new();
    fitted
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|_| Refusal::Invalid)?;
    Ok(("image/png", out, fitted.width(), fitted.height(), false))
}

/// The most an animated WebP's frames hold at once as `into_frames` reads them: each in a
/// buffer of its own, RGBA, or RGB and then a copy of it as RGBA, and beside it what decoding
/// it takes.
fn animated_need(decoder: &Webp<'_>, cost: &WebpCost) -> u64 {
    let (width, height) = decoder.dimensions();
    let n = pixels(width, height);
    let buffer = if decoder.color_type().has_alpha() {
        4 * n
    } else {
        7 * n
    };
    buffer + cost.frame
}

/// The limits an animated upload's decoder runs under, each frame as a picture.
fn animated_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(ANIMATED_SIDE);
    limits.max_image_height = Some(ANIMATED_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    limits
}

fn check_canvas((width, height): (u32, u32)) -> Result<(u32, u32), Refusal> {
    if width == 0 || height == 0 || width > ANIMATED_SIDE || height > ANIMATED_SIDE {
        Err(Refusal::TooLarge)
    } else {
        Ok((width, height))
    }
}

fn gif_decoder(bytes: &[u8]) -> Result<GifDecoder<Cursor<&[u8]>>, Refusal> {
    let mut decoder = GifDecoder::new(Cursor::new(bytes)).map_err(|_| Refusal::Invalid)?;
    decoder
        .set_limits(animated_limits())
        .map_err(|_| Refusal::TooLarge)?;
    Ok(decoder)
}

/// Every frame of an animation, its canvas checked before the first is decoded.
fn animated_frames<'a, D: AnimationDecoder<'a> + ImageDecoder>(
    decoder: D,
) -> Result<Vec<(RgbaImage, u32)>, Refusal> {
    let canvas = check_canvas(decoder.dimensions())?;
    collect_frames(decoder.into_frames(), canvas)
}

/// The frames, each composed on the whole canvas and fitted within [`EMOJI_SIDE`] as it is
/// decoded (so no more than one is held at its own size), with their delays in
/// milliseconds; refused once they pass [`ANIMATED_FRAMES`] or [`ANIMATED_PIXELS`] decoded.
fn collect_frames(
    frames: Frames<'_>,
    (width, height): (u32, u32),
) -> Result<Vec<(RgbaImage, u32)>, Refusal> {
    let per_frame = u64::from(width) * u64::from(height);
    let mut kept = Vec::new();
    let mut decoded = 0u64;
    for frame in frames {
        if kept.len() == ANIMATED_FRAMES {
            return Err(Refusal::TooLarge);
        }
        decoded += per_frame;
        if decoded > ANIMATED_PIXELS {
            return Err(Refusal::TooLarge);
        }
        let frame = frame.map_err(refusal)?;
        let (numer, denom) = frame.delay().numer_denom_ms();
        let ms = numer.checked_div(denom).unwrap_or(0);
        let ms = if ms < MIN_DELAY_MS {
            DEFAULT_DELAY_MS
        } else {
            ms
        };
        let buffer = frame.into_buffer();
        if buffer.width() == 0 || buffer.height() == 0 {
            return Err(Refusal::Invalid);
        }
        kept.push((
            fit_within(&DynamicImage::ImageRgba8(buffer), EMOJI_SIDE).to_rgba8(),
            ms,
        ));
    }
    if kept.is_empty() {
        return Err(Refusal::Invalid);
    }
    Ok(kept)
}

/// A picture someone may see through keeps its transparency as PNG; anything else is a
/// photograph and is kept as JPEG.
fn encode(image: &DynamicImage) -> Result<(&'static str, Vec<u8>), Refusal> {
    let mut out = Vec::new();
    let transparent = image.color().has_alpha() && image.to_rgba8().pixels().any(|p| p.0[3] < 255);
    if transparent {
        image
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .map_err(|_| Refusal::Invalid)?;
        Ok(("image/png", out))
    } else {
        JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .encode_image(&image.to_rgb8())
            .map_err(|_| Refusal::Invalid)?;
        Ok(("image/jpeg", out))
    }
}

/// An uploaded picture as an avatar: `{:ok, {media_type, bytes}}`, a square of at most 512
/// pixels a side encoded here, or `{:error, :unsupported | :too_large | :invalid}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn avatar<'a>(env: Env<'a>, upload: Binary<'a>) -> Term<'a> {
    let made = decode(upload.as_slice()).and_then(|image| encode(&square(&image)));
    let stored = made.and_then(|(media_type, bytes)| {
        let mut binary = OwnedBinary::new(bytes.len()).ok_or(Refusal::Invalid)?;
        binary.as_mut_slice().copy_from_slice(&bytes);
        Ok((media_type, binary.release(env)))
    });
    match stored {
        Ok(avatar) => (atoms::ok(), avatar).encode(env),
        Err(refusal) => (atoms::error(), refusal.atom()).encode(env),
    }
}

/// A page's picture as a link card keeps it: `{:ok, {media_type, bytes, width, height}}`,
/// its longest side at most 800 pixels, encoded here, or `{:error, reason}` as for an avatar.
#[rustler::nif(schedule = "DirtyCpu")]
fn card<'a>(env: Env<'a>, upload: Binary<'a>) -> Term<'a> {
    let made = decode(upload.as_slice()).and_then(|image| {
        let fitted = fit(&image);
        let (width, height) = (fitted.width(), fitted.height());
        encode(&fitted).map(|(media_type, bytes)| (media_type, bytes, width, height))
    });
    let stored = made.and_then(|(media_type, bytes, width, height)| {
        let mut binary = OwnedBinary::new(bytes.len()).ok_or(Refusal::Invalid)?;
        binary.as_mut_slice().copy_from_slice(&bytes);
        Ok((media_type, binary.release(env), width, height))
    });
    match stored {
        Ok(card) => (atoms::ok(), card).encode(env),
        Err(refusal) => (atoms::error(), refusal.atom()).encode(env),
    }
}

/// An uploaded picture as a custom emoji: `{:ok, {media_type, bytes, width, height,
/// animated}}`, within 128 pixels, a PNG or an animated GIF encoded here, or `{:error,
/// reason}` as for an avatar.
#[rustler::nif(schedule = "DirtyCpu")]
fn emoji<'a>(env: Env<'a>, upload: Binary<'a>) -> Term<'a> {
    let stored = emoji_picture(upload.as_slice()).and_then(
        |(media_type, bytes, width, height, animated)| {
            let mut binary = OwnedBinary::new(bytes.len()).ok_or(Refusal::Invalid)?;
            binary.as_mut_slice().copy_from_slice(&bytes);
            Ok((media_type, binary.release(env), width, height, animated))
        },
    );
    match stored {
        Ok(emoji) => (atoms::ok(), emoji).encode(env),
        Err(refusal) => (atoms::error(), refusal.atom()).encode(env),
    }
}

rustler::init!("Elixir.Lumia.Media.Native");

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fixture {
        ($name:literal) => {
            include_bytes!(concat!("../../../test/support/fixtures/media/", $name)).as_slice()
        };
    }

    fn made(upload: &[u8]) -> (&'static str, DynamicImage) {
        let image = decode(upload).unwrap_or_else(|_| panic!("it decodes"));
        let (media_type, bytes) = encode(&square(&image)).unwrap_or_else(|_| panic!("it encodes"));
        let read = image::load_from_memory(&bytes).expect("what was encoded reads back");
        (media_type, read)
    }

    #[test]
    fn a_photograph_stored_on_its_side_comes_out_upright_square_and_without_its_metadata() {
        let upload = fixture!("sideways.jpg");
        assert!(upload.windows(12).any(|w| w == b"LumiaTestCam"));
        let image = decode(upload).unwrap_or_else(|_| panic!("it decodes"));
        assert_eq!(
            (image.width(), image.height()),
            (320, 480),
            "turned upright"
        );
        let (media_type, bytes) = encode(&square(&image)).unwrap_or_else(|_| panic!("it encodes"));
        assert_eq!(media_type, "image/jpeg");
        assert!(
            !bytes.windows(12).any(|w| w == b"LumiaTestCam"),
            "the camera's name is gone"
        );
        assert!(
            !bytes.windows(4).any(|w| w == b"Exif"),
            "and all of its metadata"
        );
        let read = image::load_from_memory(&bytes)
            .expect("it reads back")
            .to_rgb8();
        assert_eq!((read.width(), read.height()), (320, 320));
        // Red was the left half as stored; upright it is the top.
        let (top, foot) = (read.get_pixel(160, 20).0, read.get_pixel(160, 300).0);
        assert!(top[0] > 150 && top[2] < 90, "red above: {top:?}");
        assert!(foot[2] > 150 && foot[0] < 90, "blue below: {foot:?}");
    }

    #[test]
    fn transparency_is_kept_as_png_and_anything_else_is_jpeg() {
        let (media_type, logo) = made(fixture!("logo.png"));
        assert_eq!(media_type, "image/png");
        assert_eq!(
            (logo.width(), logo.height()),
            (200, 200),
            "the middle square"
        );
        assert_eq!(
            logo.to_rgba8().get_pixel(0, 0).0[3],
            0,
            "a corner is still see-through"
        );

        let (media_type, opaque) = made(fixture!("opaque.png"));
        assert_eq!(
            media_type, "image/jpeg",
            "an alpha channel with nothing in it"
        );
        assert_eq!(
            (opaque.width(), opaque.height()),
            (AVATAR_SIDE, AVATAR_SIDE),
            "scaled down"
        );
    }

    #[test]
    fn a_small_picture_is_not_blown_up_and_other_formats_are_read() {
        let (_, small) = made(fixture!("small.png"));
        assert_eq!((small.width(), small.height()), (30, 30));
        let (media_type, frame) = made(fixture!("two-frames.gif"));
        assert_eq!(
            (media_type, frame.width(), frame.height()),
            ("image/jpeg", 80, 80)
        );
        let (_, wide) = made(fixture!("wide.webp"));
        assert_eq!((wide.width(), wide.height()), (360, 360));
    }

    #[test]
    fn what_is_too_large_broken_or_not_a_picture_is_refused_before_it_costs_anything() {
        assert!(matches!(
            decode(fixture!("claims-huge.png")),
            Err(Refusal::TooLarge)
        ));
        assert!(matches!(
            decode(fixture!("broken.png")),
            Err(Refusal::Invalid)
        ));
        assert!(matches!(
            decode(fixture!("not-a-picture.txt")),
            Err(Refusal::Unsupported)
        ));
        assert!(matches!(decode(b""), Err(Refusal::Unsupported)));
    }

    #[test]
    fn a_card_picture_keeps_its_shape_within_800_pixels() {
        let wide = fit(&DynamicImage::new_rgb8(2400, 1200));
        assert_eq!((wide.width(), wide.height()), (800, 400));
        let tall = fit(&DynamicImage::new_rgb8(600, 1800));
        assert_eq!((tall.width(), tall.height()), (267, 800));
        let small = fit(&DynamicImage::new_rgb8(100, 50));
        assert_eq!((small.width(), small.height()), (100, 50));
    }

    #[test]
    fn the_decoded_pixels_count_against_the_allocation_limit() {
        use image::codecs::png::PngEncoder;
        use image::{ExtendedColorType, ImageEncoder};
        // 64 x 64 at 16 bits a channel, RGBA: 32 KiB of pixels. A 4,096 x 6,100 picture like
        // it is within the bounds and would need 200 MB against a limit of 160 MiB.
        let samples: Vec<u8> = (0..64 * 64 * 4u32)
            .flat_map(|i| u16::try_from(i).unwrap_or(0).to_be_bytes())
            .collect();
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&samples, 64, 64, ExtendedColorType::Rgba16)
            .expect("it encodes");
        assert!(decode_within(&png, 1 << 20).is_ok());
        assert!(matches!(
            decode_within(&png, 24 * 1024),
            Err(Refusal::TooLarge)
        ));
    }

    #[test]
    fn a_picture_with_a_colour_profile_comes_out_in_srgb() {
        use image::codecs::png::PngEncoder;
        use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};
        const P3: [u8; 3] = [200, 100, 50];
        let mut srgb = [0u8; 3];
        ColorProfile::new_display_p3()
            .create_transform_8bit(
                Layout::Rgb,
                &ColorProfile::new_srgb(),
                Layout::Rgb,
                TransformOptions::default(),
            )
            .expect("a transform")
            .transform(&P3, &mut srgb)
            .expect("it converts");
        let close =
            |a: [u8; 3], within: u8| a.iter().zip(srgb).all(|(x, y)| x.abs_diff(y) <= within);
        assert!(!close(P3, 8), "the colours differ enough to tell: {srgb:?}");

        let img = RgbImage::from_pixel(16, 16, Rgb(P3));
        let encode = |icc: Option<Vec<u8>>| {
            let mut png = Vec::new();
            let mut encoder = PngEncoder::new(&mut png);
            if let Some(icc) = icc {
                encoder.set_icc_profile(icc).expect("PNG takes a profile");
            }
            encoder
                .write_image(img.as_raw(), 16, 16, ExtendedColorType::Rgb8)
                .expect("it encodes");
            png
        };
        let p3 = ColorProfile::new_display_p3().encode().expect("a profile");
        let tagged = decode(&encode(Some(p3))).unwrap_or_else(|_| panic!("it decodes"));
        assert!(close(tagged.to_rgb8().get_pixel(3, 3).0, 1));
        let plain = decode(&encode(None)).unwrap_or_else(|_| panic!("it decodes"));
        assert_eq!(plain.to_rgb8().get_pixel(3, 3).0, P3);
    }

    fn animated_gif(width: u32, height: u32, frames: u32, delay_ms: u32) -> Vec<u8> {
        use image::Rgba;
        let mut out = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut out);
            encoder.set_repeat(Repeat::Infinite).expect("repeat");
            for i in 0..frames {
                let mut pixels = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
                // A square that moves, on a see-through ground.
                let x0 = (i * 7) % (width / 2);
                for y in height / 4..height / 2 {
                    for x in x0..x0 + width / 4 {
                        pixels.put_pixel(x, y, Rgba([220, 40, (i * 30 % 255) as u8, 255]));
                    }
                }
                let delay = Delay::from_numer_denom_ms(delay_ms, 1);
                encoder
                    .encode_frame(Frame::from_parts(pixels, 0, 0, delay))
                    .expect("a frame");
            }
        }
        out
    }

    fn gif_frames(bytes: &[u8]) -> Vec<Frame> {
        GifDecoder::new(Cursor::new(bytes))
            .expect("a GIF")
            .into_frames()
            .collect_frames()
            .expect("frames")
    }

    #[test]
    fn a_still_emoji_is_a_png_within_128_with_its_shape_and_transparency() {
        let (media_type, bytes, width, height, animated) =
            emoji_picture(fixture!("logo.png")).unwrap_or_else(|_| panic!("made"));
        assert_eq!(
            (media_type, width, height, animated),
            ("image/png", 128, 85, false)
        );
        let read = image::load_from_memory(&bytes)
            .expect("reads back")
            .to_rgba8();
        assert_eq!(
            read.get_pixel(0, 0).0[3],
            0,
            "a corner is still see-through"
        );

        let (media_type, _, width, height, _) =
            emoji_picture(fixture!("sideways.jpg")).unwrap_or_else(|_| panic!("made"));
        assert_eq!(
            (media_type, width, height),
            ("image/png", 85, 128),
            "upright, shape kept"
        );

        let (_, _, width, height, _) =
            emoji_picture(fixture!("small.png")).unwrap_or_else(|_| panic!("made"));
        assert_eq!((width, height), (40, 30), "not blown up");
    }

    #[test]
    fn an_animated_gif_stays_animated_fitted_with_its_delays() {
        let upload = animated_gif(300, 200, 6, 70);
        let (media_type, bytes, width, height, animated) =
            emoji_picture(&upload).unwrap_or_else(|_| panic!("made"));
        assert_eq!(
            (media_type, width, height, animated),
            ("image/gif", 128, 85, true)
        );
        let frames = gif_frames(&bytes);
        assert_eq!(frames.len(), 6);
        for frame in &frames {
            assert_eq!(frame.delay().numer_denom_ms(), (70, 1));
            assert_eq!((frame.buffer().width(), frame.buffer().height()), (128, 85));
        }
        assert_eq!(
            frames[0].buffer().get_pixel(0, 0).0[3],
            0,
            "the ground is see-through"
        );
        assert_ne!(
            frames[0].buffer().as_raw(),
            frames[3].buffer().as_raw(),
            "the frames differ"
        );
    }

    #[test]
    fn a_gif_of_one_frame_is_a_still_png_and_a_tiny_delay_is_made_100_ms() {
        let (media_type, _, _, _, animated) =
            emoji_picture(&animated_gif(64, 64, 1, 50)).unwrap_or_else(|_| panic!("made"));
        assert_eq!((media_type, animated), ("image/png", false));
        let (_, bytes, ..) =
            emoji_picture(&animated_gif(64, 64, 3, 10)).unwrap_or_else(|_| panic!("made"));
        assert_eq!(gif_frames(&bytes)[0].delay().numer_denom_ms(), (100, 1));
    }

    #[test]
    fn an_apng_comes_out_as_an_animated_gif() {
        let (width, height, frames) = (64u32, 64u32, 4u32);
        let mut apng = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut apng, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_animated(frames, 0).expect("animated");
            encoder.set_frame_delay(1, 20).expect("a delay"); // 50 ms
            let mut writer = encoder.write_header().expect("a header");
            for i in 0..frames {
                let data: Vec<u8> = (0..width * height)
                    .flat_map(|p| [(p % 255) as u8, (i * 60) as u8, 90, 255])
                    .collect();
                writer.write_image_data(&data).expect("a frame");
            }
        }
        let (media_type, bytes, w, h, animated) =
            emoji_picture(&apng).unwrap_or_else(|_| panic!("made"));
        assert_eq!((media_type, w, h, animated), ("image/gif", 64, 64, true));
        let read = gif_frames(&bytes);
        assert_eq!(read.len(), 4);
        assert_eq!(read[0].delay().numer_denom_ms(), (50, 1));
    }

    #[test]
    fn an_animation_too_long_too_wide_or_too_large_once_made_is_refused() {
        assert!(matches!(
            emoji_picture(&animated_gif(16, 16, 201, 50)),
            Err(Refusal::TooLarge)
        ));
        assert!(emoji_picture(&animated_gif(16, 16, 200, 50)).is_ok());
        assert!(matches!(
            emoji_picture(&animated_gif(1025, 8, 2, 50)),
            Err(Refusal::TooLarge)
        ));
        // 80 frames of a full canvas is 84 million pixels decoded.
        assert!(matches!(
            emoji_picture(&animated_gif(1024, 1024, 80, 50)),
            Err(Refusal::TooLarge)
        ));
        assert!(matches!(
            emoji_picture(fixture!("claims-huge.png")),
            Err(Refusal::TooLarge)
        ));
        assert!(matches!(
            emoji_picture(fixture!("not-a-picture.txt")),
            Err(Refusal::Unsupported)
        ));
    }

    fn riff_chunk(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = fourcc.to_vec();
        out.extend_from_slice(&u32::try_from(payload.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn u24(n: u32) -> [u8; 3] {
        let b = n.to_le_bytes();
        [b[0], b[1], b[2]]
    }

    fn riff_webp(chunks: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = chunks.concat();
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&u32::try_from(body.len() + 4).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(b"WEBP");
        out.extend_from_slice(&body);
        out
    }

    fn vp8x(flags: u8, (width, height): (u32, u32)) -> Vec<u8> {
        let mut payload = vec![flags, 0, 0, 0];
        payload.extend_from_slice(&u24(width - 1));
        payload.extend_from_slice(&u24(height - 1));
        riff_chunk(b"VP8X", &payload)
    }

    /// A WebP of the frames' image chunks (`VP8 ` or `VP8L`, with an `ALPH` before a `VP8 `
    /// where it has one), each `size` and on a canvas of `canvas`, 80 ms apart.
    fn animated_webp(canvas: (u32, u32), frames: &[((u32, u32), Vec<u8>)]) -> Vec<u8> {
        let mut chunks = vec![
            vp8x(0x02 | 0x10, canvas),
            riff_chunk(b"ANIM", &[0, 0, 0, 0, 0, 0]),
        ];
        for ((width, height), image) in frames {
            let mut payload = Vec::new();
            payload.extend_from_slice(&u24(0));
            payload.extend_from_slice(&u24(0));
            payload.extend_from_slice(&u24(width - 1));
            payload.extend_from_slice(&u24(height - 1));
            payload.extend_from_slice(&u24(80));
            payload.push(0);
            payload.extend_from_slice(image);
            chunks.push(riff_chunk(b"ANMF", &payload));
        }
        riff_webp(&chunks)
    }

    /// A lossy keyframe's header claiming `width` by `height`, and bytes that are no picture.
    fn vp8_claiming(width: u16, height: u16) -> Vec<u8> {
        let first_partition = 64u32;
        let mut payload = u24((first_partition << 5) | 0x10).to_vec();
        payload.extend_from_slice(&[0x9d, 0x01, 0x2a]);
        payload.extend_from_slice(&width.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        payload.extend(std::iter::repeat_n(0x55, 64));
        riff_chunk(b"VP8 ", &payload)
    }

    /// The image chunk of a still WebP of one chunk (`VP8 ` or `VP8L`).
    fn image_chunk(webp: &[u8]) -> Vec<u8> {
        webp[12..].to_vec()
    }

    /// A lossless WebP of `pixels`, as `image` writes one.
    fn lossless(pixels: &RgbaImage) -> Vec<u8> {
        let mut out = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut out)
            .encode(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
                image::ExtendedColorType::Rgba8,
            )
            .expect("it encodes");
        out
    }

    #[test]
    fn an_animated_webp_lossy_or_lossless_comes_out_as_an_animated_gif() {
        let lossy = image_chunk(fixture!("lossy.webp"));
        let upload = animated_webp((40, 30), &[((40, 30), lossy.clone()), ((40, 30), lossy)]);
        let (media_type, bytes, width, height, animated) =
            emoji_picture(&upload).unwrap_or_else(|_| panic!("made"));
        assert_eq!(
            (media_type, width, height, animated),
            ("image/gif", 40, 30, true)
        );
        assert_eq!(gif_frames(&bytes).len(), 2);

        use image::Rgba;
        let frames: Vec<_> = (0..3u8)
            .map(|i| {
                let pixels = RgbaImage::from_pixel(20, 10, Rgba([i * 80, 10, 200, 255]));
                ((20, 10), image_chunk(&lossless(&pixels)))
            })
            .collect();
        let (media_type, bytes, ..) =
            emoji_picture(&animated_webp((20, 10), &frames)).unwrap_or_else(|_| panic!("made"));
        assert_eq!(media_type, "image/gif");
        let read = gif_frames(&bytes);
        assert_eq!(read.len(), 3);
        assert_eq!(read[0].delay().numer_denom_ms(), (80, 1));
    }

    #[test]
    fn a_lossy_frame_that_claims_more_than_its_frame_is_refused_before_it_is_decoded() {
        // Frames of 16 by 16 on a canvas of 16 by 16, whose bitstreams each say 16,383 by
        // 16,383: the decoder would allocate for that (some 400 MB) before it compared the
        // two.
        let claims = vp8_claiming(16383, 16383);
        let upload = animated_webp((16, 16), &[((16, 16), claims.clone()), ((16, 16), claims)]);
        assert!(matches!(emoji_picture(&upload), Err(Refusal::TooLarge)));

        // After an alpha chunk, as a lossy frame with transparency has it.
        let alph = riff_chunk(b"ALPH", &[0; 17]);
        let framed = [alph, vp8_claiming(16383, 16383)].concat();
        let upload = animated_webp((16, 16), &[((16, 16), framed.clone()), ((16, 16), framed)]);
        assert!(matches!(emoji_picture(&upload), Err(Refusal::TooLarge)));

        // A still one, as an avatar or a card's picture would read it: the canvas is what
        // the header says, and the bitstream inside says otherwise.
        let still = riff_webp(&[vp8x(0, (16, 16)), vp8_claiming(16383, 16383)]);
        assert!(matches!(decode(&still), Err(Refusal::TooLarge)));
        assert!(matches!(emoji_picture(&still), Err(Refusal::TooLarge)));

        // The same container with a bitstream that fits it decodes.
        let lossy = image_chunk(fixture!("lossy.webp"));
        let fits = riff_webp(&[vp8x(0, (40, 30)), lossy.clone()]);
        let image = decode(&fits).unwrap_or_else(|_| panic!("it decodes"));
        assert_eq!((image.width(), image.height()), (40, 30));

        // A colour profile claiming 4 GB, which the decoder would allocate before it found
        // the upload ends.
        let mut claims = riff_chunk(b"ICCP", &[0; 8]);
        claims[4..8].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        let profile = riff_webp(&[vp8x(0x20, (40, 30)), lossy, claims]);
        assert!(matches!(decode(&profile), Err(Refusal::Invalid)));
    }

    /// A lossless bitstream's header claiming `width` by `height`, and bytes that are no picture.
    fn vp8l_claiming(width: u32, height: u32, alpha: bool) -> Vec<u8> {
        let bits = (width - 1) | (height - 1) << 14 | u32::from(alpha) << 28;
        let mut payload = vec![0x2f];
        payload.extend_from_slice(&bits.to_le_bytes());
        payload.extend(std::iter::repeat_n(0x55, 64));
        riff_chunk(b"VP8L", &payload)
    }

    /// The first chunk of `webp` with the code `fourcc`, header and all.
    fn chunk_of(webp: &[u8], fourcc: &[u8; 4]) -> Vec<u8> {
        RiffChunks {
            bytes: webp,
            at: 12,
        }
        .filter_map(Result::ok)
        .find(|(code, _)| code == fourcc)
        .map(|(code, payload)| riff_chunk(&code, payload))
        .expect("the chunk")
    }

    /// The most `run` held allocated at once on this thread, in bytes.
    fn peak(run: impl FnOnce()) -> u64 {
        allocation_counter::measure(run).bytes_max
    }

    /// What decoding `upload` under `max_alloc` really held at most, and what it reserved.
    fn measured(upload: &[u8], max_alloc: u64) -> (u64, u64) {
        let mut reserved = 0;
        let held = peak(|| {
            let (image, need) =
                decode_within(upload, max_alloc).unwrap_or_else(|_| panic!("it decodes"));
            reserved = need;
            drop(image);
        });
        (held, reserved)
    }

    #[test]
    fn a_webp_with_alpha_past_what_fits_is_refused_before_it_is_decoded() {
        // A lossy picture with a lossless alpha plane, as cwebp makes one by default, holds
        // some 11.4 bytes a pixel decoding: 3,836 a side is the largest square that fits.
        let lossless_alpha = |side: u32| {
            riff_webp(&[
                vp8x(0x10, (side, side)),
                riff_chunk(b"ALPH", &[0x01; 64]),
                vp8_claiming(side as u16, side as u16),
            ])
        };
        for side in [3837, 5000] {
            let mut refused = None;
            let held = peak(|| refused = Some(decode(&lossless_alpha(side))));
            assert!(matches!(refused, Some(Err(Refusal::TooLarge))), "{side}");
            assert!(held < 64 * 1024, "refused before it is decoded: {held}");
        }
        let fits = fixture!("alpha-3836.webp");
        let (image, reserved) =
            decode_within(fits, MAX_ALLOC).unwrap_or_else(|_| panic!("it decodes"));
        assert_eq!((image.width(), image.height()), (3836, 3836));
        assert!(image.color().has_alpha());
        assert!(reserved <= MAX_ALLOC);

        // A lossless picture without alpha is decoded as RGBA and copied out: 7.9 bytes a
        // pixel, so 4,611 a side. With alpha it is decoded in place and any size fits.
        let lossless = |side: u32, alpha: bool| riff_webp(&[vp8l_claiming(side, side, alpha)]);
        assert!(matches!(
            decode(&lossless(4612, false)),
            Err(Refusal::TooLarge)
        ));
        assert!(matches!(
            decode(&lossless(4611, false)),
            Err(Refusal::Invalid)
        ));
        assert!(matches!(
            decode(&lossless(5000, true)),
            Err(Refusal::Invalid)
        ));

        // A raw alpha plane is a byte a pixel, and an opaque lossy picture has none: both
        // fit at the bounds.
        let raw = riff_webp(&[
            vp8x(0x10, (5000, 5000)),
            riff_chunk(b"ALPH", &[0x00; 64]),
            vp8_claiming(5000, 5000),
        ]);
        assert!(matches!(decode(&raw), Err(Refusal::Invalid)));
        assert!(matches!(
            decode(&riff_webp(&[vp8_claiming(5000, 5000)])),
            Err(Refusal::Invalid)
        ));
    }

    #[test]
    fn what_a_webp_decode_reserves_is_at_least_what_it_allocates() {
        // Each path image-webp takes, at the largest that fits where there is a limit, decoded
        // with a counting allocator: if a new version of the decoder allocates more than is
        // reserved for it, this fails and not the instance.
        let lossy_alpha = fixture!("alpha-3836.webp");
        let (alph, lossy) = (
            chunk_of(lossy_alpha, b"ALPH"),
            chunk_of(lossy_alpha, b"VP8 "),
        );
        let side = 3836u32;
        let raw_alph = {
            let mut payload = vec![0x00];
            payload.extend(std::iter::repeat_n(0xC0, (side * side) as usize));
            riff_chunk(b"ALPH", &payload)
        };
        let lossless_opaque = fixture!("lossless-4600.webp");
        let vp8l = chunk_of(lossless_opaque, b"VP8L");
        let wide = (4600u32, 4600u32);
        let lossy_frame = [alph.clone(), lossy.clone()].concat();
        let cases: Vec<(&str, Vec<u8>, u64)> = vec![
            ("lossy, lossless alpha", lossy_alpha.to_vec(), MAX_ALLOC),
            (
                "lossy, raw alpha",
                riff_webp(&[vp8x(0x10, (side, side)), raw_alph, lossy.clone()]),
                MAX_ALLOC,
            ),
            ("lossy", riff_webp(std::slice::from_ref(&lossy)), MAX_ALLOC),
            (
                "lossy in an extended file",
                riff_webp(&[vp8x(0, (side, side)), lossy.clone()]),
                MAX_ALLOC,
            ),
            ("lossless", lossless_opaque.to_vec(), MAX_ALLOC),
            (
                "lossless, alpha",
                riff_webp(&[vp8x(0x10, wide), vp8l.clone()]),
                MAX_ALLOC,
            ),
            (
                "lossless in an extended file",
                riff_webp(&[vp8x(0, wide), vp8l.clone()]),
                MAX_ALLOC,
            ),
            // An animation's first frame, past what fits once its canvas is counted, so
            // measured without the limit.
            (
                "animated, lossy frames with alpha",
                animated_webp(
                    (side, side),
                    &[
                        ((side, side), lossy_frame.clone()),
                        ((side, side), lossy_frame),
                    ],
                ),
                u64::MAX,
            ),
            (
                "animated, lossy frames",
                animated_webp(
                    (side, side),
                    &[((side, side), lossy.clone()), ((side, side), lossy.clone())],
                ),
                u64::MAX,
            ),
            (
                "animated, lossless frames",
                animated_webp(wide, &[(wide, vp8l.clone()), (wide, vp8l.clone())]),
                u64::MAX,
            ),
        ];
        for (kind, upload, max_alloc) in &cases {
            let (held, reserved) = measured(upload, *max_alloc);
            eprintln!("{kind}: held {held}, reserved {reserved}");
            assert!(
                held <= reserved,
                "{kind}: held {held} bytes, reserved {reserved}"
            );
        }
    }

    #[test]
    fn a_lossless_webp_with_more_prefix_codes_than_fit_is_refused_as_they_pass_the_limit() {
        // 4 by 4 pixels in 42 KB, but its entropy image names the last of 4,096 prefix-code
        // groups, each with a code of 2,048 symbols: about 286 MB of tables, which image-webp
        // 0.2.4 allocated whatever its memory limit said. Their number is known only from the
        // entropy image and each one's size only as it is read, so they are counted as they
        // are read, and refused at the first that would pass what the decode has left.
        let upload = fixture!("prefix-codes.webp");
        let limited = |max_alloc: u64| {
            let mut refused = None;
            let held = peak(|| refused = Some(decode_within(upload, max_alloc).map(|_| ())));
            assert!(
                matches!(refused, Some(Err(Refusal::TooLarge))),
                "refused under {max_alloc}"
            );
            held
        };
        let held = limited(4 << 20);
        assert!(held <= 4 << 20, "held {held} bytes under 4 MiB");
        let held = limited(MAX_ALLOC);
        assert!(held <= MAX_ALLOC, "held {held} bytes");
        assert!(matches!(emoji_picture(upload), Err(Refusal::TooLarge)));

        // Without a limit it is a picture.
        let (image, _) = decode_within(upload, u64::MAX).unwrap_or_else(|_| panic!("decodes"));
        assert_eq!((image.width(), image.height()), (4, 4));
    }

    #[test]
    fn a_quarter_turn_counts_its_copy() {
        // An EXIF chunk saying the picture is on its side (orientation 6).
        let mut exif = b"MM\0\x2a\0\0\0\x08\0\x01".to_vec();
        exif.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, 0, 0, 0, 0]);
        let vp8l = chunk_of(fixture!("lossless-4600.webp"), b"VP8L");
        let upright = riff_webp(&[vp8x(0x10, (4600, 4600)), vp8l.clone()]);
        let turned = riff_webp(&[
            vp8x(0x10 | 0x08, (4600, 4600)),
            vp8l,
            riff_chunk(b"EXIF", &exif),
        ]);
        assert!(decode_within(&upright, MAX_ALLOC).is_ok());
        assert!(matches!(
            decode_within(&turned, MAX_ALLOC),
            Err(Refusal::TooLarge)
        ));
        let (held, reserved) = measured(&turned, u64::MAX);
        assert!(held <= reserved, "held {held} bytes, reserved {reserved}");
    }

    #[test]
    fn what_an_animated_webp_reserves_for_its_frames_is_at_least_what_they_take() {
        // Every frame of an animation, as an emoji's are read, on a canvas far past what an
        // emoji may have, so the slope is seen.
        let lossy_alpha = fixture!("alpha-3836.webp");
        let frame = [
            chunk_of(lossy_alpha, b"ALPH"),
            chunk_of(lossy_alpha, b"VP8 "),
        ]
        .concat();
        let lossy = chunk_of(lossy_alpha, b"VP8 ");
        let side = (3836, 3836);
        let opaque = {
            let mut upload = animated_webp(side, &[(side, lossy.clone()), (side, lossy.clone())]);
            // The canvas's alpha flag cleared: each frame comes out as RGB, then RGBA.
            upload[20] &= !0x10;
            upload
        };
        for (kind, upload) in [
            (
                "lossy with alpha",
                animated_webp(side, &[(side, frame.clone()), (side, frame)]),
            ),
            (
                "lossy, the canvas with alpha",
                animated_webp(side, &[(side, lossy.clone()), (side, lossy)]),
            ),
            ("lossy, the canvas without alpha", opaque),
        ] {
            let cost = webp_cost(&upload).unwrap_or_else(|_| panic!("a WebP"));
            let decoder = Webp::new(&upload).unwrap_or_else(|_| panic!("a WebP"));
            let need = animated_need(&decoder, &cost);
            let held = peak(|| {
                for frame in decoder.into_frames() {
                    drop(frame.expect("a frame"));
                }
            });
            eprintln!("{kind}: held {held}, reserved {need}");
            assert!(held <= need, "{kind}: held {held} bytes, reserved {need}");
        }
    }

    #[test]
    fn what_is_animated_is_told_by_its_frames_before_any_bounds_apply() {
        // One frame is a still picture, held to a still picture's bounds: 1,026 wide fits.
        let (media_type, _, width, _, animated) =
            emoji_picture(&animated_gif(1026, 8, 1, 50)).unwrap_or_else(|_| panic!("made"));
        assert_eq!((media_type, width, animated), ("image/png", 128, false));

        let pixels = RgbaImage::new(1026, 8);
        let one = animated_webp((1026, 8), &[((1026, 8), image_chunk(&lossless(&pixels)))]);
        let (media_type, _, width, _, animated) =
            emoji_picture(&one).unwrap_or_else(|_| panic!("made"));
        assert_eq!((media_type, width, animated), ("image/png", 128, false));

        let apng = |width: u32, frames: u32| {
            let mut out = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut out, width, 8);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_animated(frames, 0).expect("animated");
                let mut writer = encoder.write_header().expect("a header");
                for i in 0..frames {
                    let data = vec![u8::try_from(i * 40).unwrap_or(0); (width * 8 * 4) as usize];
                    writer.write_image_data(&data).expect("a frame");
                }
            }
            out
        };
        let (media_type, _, _, _, animated) =
            emoji_picture(&apng(1026, 1)).unwrap_or_else(|_| panic!("made"));
        assert_eq!((media_type, animated), ("image/png", false));

        // More than one is animated, held to an animation's canvas: 1,025 wide does not.
        assert!(matches!(
            emoji_picture(&apng(1025, 2)),
            Err(Refusal::TooLarge)
        ));
        assert!(matches!(
            emoji_picture(&animated_webp(
                (1025, 8),
                &[
                    ((1025, 8), image_chunk(&lossless(&RgbaImage::new(1025, 8)))),
                    ((1025, 8), image_chunk(&lossless(&RgbaImage::new(1025, 8)))),
                ]
            )),
            Err(Refusal::TooLarge)
        ));
        assert!(matches!(
            emoji_picture(&animated_gif(1025, 8, 2, 50)),
            Err(Refusal::TooLarge)
        ));
    }

    #[test]
    fn noise_that_will_not_compress_is_refused_past_512_kib() {
        use image::Rgba;
        let mut out = Vec::new();
        {
            let mut encoder = GifEncoder::new_with_speed(&mut out, 30);
            let mut seed = 7u32;
            for _ in 0..60 {
                let pixels = RgbaImage::from_fn(128, 128, |_, _| {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    let b = seed.to_be_bytes();
                    Rgba([b[0], b[1], b[2], 255])
                });
                encoder
                    .encode_frame(Frame::from_parts(
                        pixels,
                        0,
                        0,
                        Delay::from_numer_denom_ms(50, 1),
                    ))
                    .expect("a frame");
            }
        }
        assert!(matches!(emoji_picture(&out), Err(Refusal::TooLarge)));
    }
}
