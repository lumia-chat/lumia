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

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
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
const JPEG_QUALITY: u8 = 85;

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

fn reader(bytes: &[u8]) -> Result<(ImageReader<Cursor<&[u8]>>, ImageFormat), Refusal> {
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

/// The upload as pixels, upright: bounds first, then the decode under its limits, then
/// the turn its metadata asks for (a phone's photo is stored on its side and says so).
fn decode(bytes: &[u8]) -> Result<DynamicImage, Refusal> {
    let (probe, _) = reader(bytes)?;
    let (width, height) = probe.into_dimensions().map_err(|_| Refusal::Invalid)?;
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(Refusal::TooLarge);
    }
    let (mut reader, _) = reader(bytes)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| Refusal::Invalid)?;
    let orientation = decoder.orientation().map_err(|_| Refusal::Invalid)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| Refusal::Invalid)?;
    image.apply_orientation(orientation);
    Ok(image)
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
}
