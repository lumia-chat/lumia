//! Still pictures: the bounded decode, the display image and its placeholder
//! (specs/media.md 3.1), and the thumbnail of a GIF's first frame, a video's
//! poster and a card's picture.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::{self, FilterType};
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits,
    RgbImage, RgbaImage,
};
use moxcms::{
    ColorProfile, DataColorSpace, Layout, ParsingOptions, TransformExecutor, TransformOptions,
};

use super::detect::{probe, Format, MAX_SIDE};
use super::{MediaError, TAG_BYTES};

/// What the decoder may allocate: the most pixels allowed as RGBA, and
/// room to work.
const MAX_ALLOC: u64 = 192 * 1024 * 1024;
/// The display image's longest edge.
pub(super) const DISPLAY_EDGE: u32 = 2560;
/// The smallest longest edge a display image is shrunk to before the
/// picture is refused.
pub(super) const MIN_DISPLAY_EDGE: u32 = 640;
/// A thumbnail's longest edge (a GIF's first frame, a video's poster): a
/// picture across a phone's column (some 360 to 430 points) on a display of
/// three pixels a point, and a desktop's at two.
pub(super) const THUMBNAIL_EDGE: u32 = 1080;
/// The longest edge of a card's picture (spec 6.3).
pub(super) const CARD_EDGE: u32 = 800;
/// The longest edge of what the placeholder is computed from (ThumbHash
/// takes at most 100 a side).
const PLACEHOLDER_EDGE: u32 = 100;
const JPEG_QUALITIES: [u8; 4] = [85, 75, 65, 55];
const THUMBNAIL_QUALITIES: [u8; 4] = [85, 75, 65, 55];

/// A picture made ready to send.
#[derive(Debug)]
pub(super) struct Still {
    pub main: Vec<u8>,
    pub png: bool,
    pub width: u32,
    pub height: u32,
    pub placeholder: Vec<u8>,
}

/// A thumbnail and its placeholder.
#[derive(Debug)]
pub(super) struct Preview {
    pub thumbnail: Vec<u8>,
    pub placeholder: Vec<u8>,
}

/// `bytes` decoded, upright and in sRGB, or `None` when they are not a
/// picture the core decodes, declare a size past the bounds, or will not
/// decode under the allocation limit. The first frame of an animated GIF.
pub(super) fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    decode_within(bytes, MAX_ALLOC)
}

/// [`decode`] under an allocation limit of `max_alloc` bytes.
pub(super) fn decode_within(bytes: &[u8], max_alloc: u64) -> Option<DynamicImage> {
    let probe = probe(bytes)?;
    let format = match probe.format {
        Format::Jpeg => ImageFormat::Jpeg,
        Format::Png => ImageFormat::Png,
        Format::Gif => ImageFormat::Gif,
        Format::WebP => ImageFormat::WebP,
        _ => return None,
    };
    if !probe.within_bounds() {
        return None;
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(max_alloc);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().ok()?;
    // The decoded pixels count against the limit before they are allocated,
    // as `ImageReader::decode` counts them: a picture within the bounds can
    // still need more (16 bits a channel), and `from_decoder` does not ask.
    limits.reserve(decoder.total_bytes()).ok()?;
    decoder.set_limits(limits).ok()?;
    // An orientation that will not read is no turn, not a broken picture;
    // nor is a colour profile that will not.
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let icc = decoder.icc_profile().ok().flatten();
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    if let Some(icc) = icc {
        to_srgb(&mut image, &icc);
    }
    image.apply_orientation(orientation);
    Some(image)
}

/// The largest embedded colour profile read: a display's is a few hundred
/// bytes, a printer's with its lookup tables a few hundred kilobytes.
const MAX_ICC_BYTES: usize = 1 << 20;
/// The largest colour lookup table a profile may build, in entries.
const MAX_CLUT: usize = 1 << 20;

/// Converts, in place, pixels in the colour space an embedded ICC profile
/// names (Display P3 from a phone, Adobe RGB from a camera) to sRGB, which
/// is what a picture without a profile is taken to be: the profile itself
/// is not kept (spec 3.1). A profile that does not parse or is not RGB,
/// and pixels that are not RGB of 8 or 16 bits, are left as they are; a
/// grey picture is grey in any profile.
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

/// `samples` run through `transform` a row of `row` samples at a time, so
/// the copy it needs is one row and not the picture.
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

/// Platform-decoded pixels (RGBA8, upright) as an image.
///
/// Held to the same bounds as a decode of the core's own: a shell that
/// hands over a larger picture has decoded more than any attachment may be,
/// and resizing it would cost what the bounds exist to refuse.
pub(super) fn from_pixels(rgba: Vec<u8>, width: u32, height: u32) -> Result<RgbaImage, MediaError> {
    if !super::within_bounds(width, height) {
        return Err(MediaError::Pixels);
    }
    RgbaImage::from_raw(width, height, rgba).ok_or(MediaError::Pixels)
}

/// The size of `(width, height)` with its longest edge at most `edge`,
/// never enlarged and never zero.
fn fit(width: u32, height: u32, edge: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= edge {
        return (width, height);
    }
    let scale = |side: u32| {
        let scaled = u64::from(side) * u64::from(edge) / u64::from(longest);
        u32::try_from(scaled).unwrap_or(edge).max(1)
    };
    (scale(width), scale(height))
}

/// A decoded picture with its longest edge brought within `edge`, as RGBA.
pub(super) fn shrink_decoded(image: &DynamicImage, edge: u32) -> RgbaImage {
    let (w, h) = fit(image.width(), image.height(), edge);
    if (w, h) == (image.width(), image.height()) {
        image.to_rgba8()
    } else {
        image.resize_exact(w, h, FilterType::CatmullRom).to_rgba8()
    }
}

/// RGBA brought within the display edge.
pub(super) fn shrink(image: RgbaImage) -> RgbaImage {
    let (w, h) = fit(image.width(), image.height(), DISPLAY_EDGE);
    if (w, h) == image.dimensions() {
        image
    } else {
        imageops::resize(&image, w, h, FilterType::CatmullRom)
    }
}

fn transparent(image: &RgbaImage) -> bool {
    image.pixels().any(|p| p.0[3] < 255)
}

/// RGBA flattened onto white, for JPEG.
fn flatten(image: &RgbaImage) -> RgbImage {
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let [r, g, b, a] = image.get_pixel(x, y).0;
        let over = |c: u8| {
            let (c, a) = (u16::from(c), u16::from(a));
            u8::try_from((c * a + 255 * (255 - a) + 127) / 255).unwrap_or(255)
        };
        image::Rgb([over(r), over(g), over(b)])
    })
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, MediaError> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| MediaError::Encode(e.to_string()))?;
    Ok(out)
}

fn encode_jpeg(image: &RgbImage, quality: u8) -> Result<Vec<u8>, MediaError> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .encode(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| MediaError::Encode(e.to_string()))?;
    Ok(out)
}

/// Whether `len` plaintext bytes seal to at most `cap` bytes.
fn fits(len: usize, cap: u64) -> bool {
    (len as u64).saturating_add(TAG_BYTES as u64) <= cap
}

/// The display image of an upright picture already within the display
/// edge: PNG when it has transparency and the PNG fits under `cap`;
/// otherwise JPEG at 85 stepping down by 10 to 55, then the longest edge
/// down by a quarter at a time (at 55) until it fits. A picture that does
/// not fit at [`MIN_DISPLAY_EDGE`] is refused. A transparent picture made
/// JPEG is flattened onto white.
pub(super) fn display(image: RgbaImage, cap: u64) -> Result<Still, MediaError> {
    if transparent(&image) {
        let png = encode_png(&image)?;
        if fits(png.len(), cap) {
            let (width, height) = image.dimensions();
            return Ok(Still {
                main: png,
                png: true,
                width,
                height,
                placeholder: placeholder(&DynamicImage::ImageRgba8(image)),
            });
        }
    }
    let rgb = flatten(&image);
    drop(image);
    for quality in JPEG_QUALITIES {
        let jpeg = encode_jpeg(&rgb, quality)?;
        if fits(jpeg.len(), cap) {
            return Ok(jpeg_still(jpeg, rgb));
        }
    }
    let mut edge = rgb.width().max(rgb.height());
    loop {
        if edge <= MIN_DISPLAY_EDGE {
            return Err(MediaError::ImageTooLarge { cap });
        }
        edge = (edge - edge / 4).max(MIN_DISPLAY_EDGE);
        let (w, h) = fit(rgb.width(), rgb.height(), edge);
        let smaller = imageops::resize(&rgb, w, h, FilterType::CatmullRom);
        let jpeg = encode_jpeg(&smaller, 55)?;
        if fits(jpeg.len(), cap) {
            return Ok(jpeg_still(jpeg, smaller));
        }
    }
}

fn jpeg_still(main: Vec<u8>, shown: RgbImage) -> Still {
    let (width, height) = shown.dimensions();
    Still {
        main,
        png: false,
        width,
        height,
        placeholder: placeholder(&DynamicImage::ImageRgb8(shown)),
    }
}

/// The placeholder of an upright picture: the ThumbHash of it at
/// [`PLACEHOLDER_EDGE`].
fn placeholder(image: &DynamicImage) -> Vec<u8> {
    let (pw, ph) = fit(image.width(), image.height(), PLACEHOLDER_EDGE);
    let tiny = image.thumbnail_exact(pw, ph).to_rgba8();
    thumbhash::rgba_to_thumb_hash(pw as usize, ph as usize, tiny.as_raw())
}

/// The thumbnail (at most `edge`, JPEG at 85 or PNG when `png`, within
/// `cap` sealed) and the placeholder of an upright picture: a GIF's first
/// frame, a video's poster, a card's picture. A thumbnail over the limit
/// steps down in quality, then in size, as the display image does.
pub(super) fn preview(
    image: &DynamicImage,
    png: bool,
    edge: u32,
    cap: u64,
) -> Result<Preview, MediaError> {
    let (w, h) = fit(image.width(), image.height(), edge);
    let small = image.thumbnail_exact(w, h).to_rgba8();
    let (pw, ph) = fit(w, h, PLACEHOLDER_EDGE);
    let tiny = imageops::thumbnail(&small, pw, ph);
    let placeholder = thumbhash::rgba_to_thumb_hash(pw as usize, ph as usize, tiny.as_raw());
    if png {
        let encoded = encode_png(&small)?;
        if fits(encoded.len(), cap) {
            return Ok(Preview {
                thumbnail: encoded,
                placeholder,
            });
        }
    }
    let rgb = flatten(&small);
    for quality in THUMBNAIL_QUALITIES {
        let encoded = encode_jpeg(&rgb, quality)?;
        if fits(encoded.len(), cap) {
            return Ok(Preview {
                thumbnail: encoded,
                placeholder,
            });
        }
    }
    let mut edge = w.max(h);
    while edge > PLACEHOLDER_EDGE {
        edge = (edge - edge / 4).max(PLACEHOLDER_EDGE);
        let (tw, th) = fit(w, h, edge);
        let encoded = encode_jpeg(&imageops::thumbnail(&rgb, tw, th), 50)?;
        if fits(encoded.len(), cap) {
            return Ok(Preview {
                thumbnail: encoded,
                placeholder,
            });
        }
    }
    Err(MediaError::ThumbnailTooLarge)
}
