#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;

use image::codecs::gif::{GifEncoder, Repeat};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, Frame, ImageEncoder, Rgb, RgbImage, Rgba, RgbaImage};

use super::*;

// ---------------------------------------------------------------------------
// Fixtures, built here rather than checked in.

fn input(bytes: Vec<u8>, name: &str) -> Input {
    Input {
        bytes,
        name: name.into(),
        ..Input::default()
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A JPEG `w` x `h`, its left half red and its right half blue.
fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = RgbImage::from_fn(w, h, |x, _| {
        if x < w / 2 {
            Rgb([255, 0, 0])
        } else {
            Rgb([0, 0, 255])
        }
    });
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&img)
        .unwrap();
    out
}

/// An APP1 EXIF segment with IFD0 holding an orientation and a pointer to
/// a GPS IFD naming a latitude.
fn exif_segment(orientation: u16) -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(b"MM\0*");
    t.extend_from_slice(&8u32.to_be_bytes());
    // IFD0 at 8: two entries.
    t.extend_from_slice(&2u16.to_be_bytes());
    t.extend_from_slice(&0x0112u16.to_be_bytes());
    t.extend_from_slice(&3u16.to_be_bytes());
    t.extend_from_slice(&1u32.to_be_bytes());
    t.extend_from_slice(&orientation.to_be_bytes());
    t.extend_from_slice(&[0, 0]);
    t.extend_from_slice(&0x8825u16.to_be_bytes());
    t.extend_from_slice(&4u16.to_be_bytes());
    t.extend_from_slice(&1u32.to_be_bytes());
    t.extend_from_slice(&38u32.to_be_bytes());
    t.extend_from_slice(&0u32.to_be_bytes());
    // GPS IFD at 38: GPSLatitudeRef "N", and a string past it.
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&0x0001u16.to_be_bytes());
    t.extend_from_slice(&2u16.to_be_bytes());
    t.extend_from_slice(&2u32.to_be_bytes());
    t.extend_from_slice(b"N\0\0\0");
    t.extend_from_slice(&0u32.to_be_bytes());
    t.extend_from_slice(b"SECRET-GPS");
    segment(0xE1, &[b"Exif\0\0".as_slice(), &t].concat())
}

fn segment(marker: u8, body: &[u8]) -> Vec<u8> {
    let mut s = vec![0xFF, marker];
    s.extend_from_slice(&u16::try_from(body.len() + 2).unwrap().to_be_bytes());
    s.extend_from_slice(body);
    s
}

/// A JPEG turned by EXIF orientation 6, with GPS, XMP, a comment and an
/// ICC profile.
fn camera_jpeg(w: u32, h: u32) -> Vec<u8> {
    let plain = jpeg(w, h);
    let mut out = plain[..2].to_vec();
    out.extend(exif_segment(6));
    out.extend(segment(0xE1, b"http://ns.adobe.com/xap/1.0/\0SECRET-XMP"));
    out.extend(segment(0xE2, b"ICC_PROFILE\0\x01\x01profile"));
    out.extend(segment(0xFE, b"SECRET-COM"));
    out.extend_from_slice(&plain[2..]);
    out.extend_from_slice(b"SECRET-TRAILER");
    out
}

/// The markers of a JPEG's segments before its first scan.
fn jpeg_markers(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pos = 2;
    while bytes[pos] == 0xFF && bytes[pos + 1] != 0xDA {
        let len = usize::from(u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]));
        out.push((bytes[pos + 1], bytes[pos + 4..pos + 2 + len].to_vec()));
        pos += 2 + len;
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut c = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
    c.extend_from_slice(kind);
    c.extend_from_slice(data);
    c.extend_from_slice(&crc32(&[kind.as_slice(), data].concat()).to_be_bytes());
    c
}

fn png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .unwrap();
    out
}

/// A PNG with text and a time inserted before IEND.
fn png_with_text() -> Vec<u8> {
    let plain = png(&RgbaImage::from_pixel(8, 8, Rgba([10, 200, 30, 255])));
    let iend = plain.len() - 12;
    let mut out = plain[..iend].to_vec();
    out.extend(png_chunk(b"tEXt", b"Comment\0SECRET-TEXT"));
    out.extend(png_chunk(b"tIME", &[0x07, 0xE9, 1, 2, 3, 4, 5]));
    out.extend_from_slice(&plain[iend..]);
    out
}

fn gif(frames: usize) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut out);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        let frames = (0..frames).map(|i| {
            let shade = u8::try_from(i * 100).unwrap();
            Frame::new(RgbaImage::from_pixel(8, 6, Rgba([shade, 50, 50, 255])))
        });
        encoder.encode_frames(frames).unwrap();
    }
    out
}

/// A GIF with a comment and an XMP application extension after its
/// header.
fn gif_with_comment(frames: usize) -> (Vec<u8>, usize) {
    let plain = gif(frames);
    let packed = plain[10];
    let at = 13
        + if packed & 0x80 == 0 {
            0
        } else {
            3 << ((packed & 7) + 1)
        };
    let mut extra = vec![0x21, 0xFE, 14];
    extra.extend_from_slice(b"SECRET-COMMENT");
    extra.push(0);
    extra.extend_from_slice(&[0x21, 0xFF, 11]);
    extra.extend_from_slice(b"XMP DataXMP");
    extra.push(10);
    extra.extend_from_slice(b"SECRET-XMP");
    extra.push(0);
    let mut out = plain[..at].to_vec();
    out.extend_from_slice(&extra);
    out.extend_from_slice(&plain[at..]);
    (out, extra.len())
}

fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut b = u32::try_from(payload.len() + 8)
        .unwrap()
        .to_be_bytes()
        .to_vec();
    b.extend_from_slice(kind);
    b.extend_from_slice(payload);
    b
}

fn full(kind: &[u8; 4], version: u8, payload: &[u8]) -> Vec<u8> {
    bx(kind, &[&[version, 0, 0, 0][..], payload].concat())
}

const IDENTITY: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];
const QUARTER: [u32; 9] = [0, 0x0001_0000, 0, 0xFFFF_0000, 0, 0, 0, 0, 0x4000_0000];

fn mvhd() -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0x1111_1111u32.to_be_bytes());
    p.extend_from_slice(&0x2222_2222u32.to_be_bytes());
    p.extend_from_slice(&600u32.to_be_bytes());
    p.extend_from_slice(&3000u32.to_be_bytes()); // 5 s
    p.extend_from_slice(&[0; 80]);
    full(b"mvhd", 0, &p)
}

fn tkhd(matrix: [u32; 9]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0x3333_3333_3333_3333u64.to_be_bytes());
    p.extend_from_slice(&0x4444_4444_4444_4444u64.to_be_bytes());
    p.extend_from_slice(&1u32.to_be_bytes());
    p.extend_from_slice(&[0; 4]);
    p.extend_from_slice(&3000u64.to_be_bytes());
    p.extend_from_slice(&[0; 16]);
    for m in matrix {
        p.extend_from_slice(&m.to_be_bytes());
    }
    p.extend_from_slice(&(1920u32 << 16).to_be_bytes());
    p.extend_from_slice(&(1080u32 << 16).to_be_bytes());
    full(b"tkhd", 1, &p)
}

fn mdhd() -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0x5555_5555u32.to_be_bytes());
    p.extend_from_slice(&0x6666_6666u32.to_be_bytes());
    p.extend_from_slice(&600u32.to_be_bytes());
    p.extend_from_slice(&3000u32.to_be_bytes());
    p.extend_from_slice(&[0x55, 0xC4, 0, 0]);
    full(b"mdhd", 0, &p)
}

fn hdlr(handler: &[u8; 4]) -> Vec<u8> {
    full(b"hdlr", 0, &[&[0; 4][..], handler, &[0; 13]].concat())
}

/// A movie: ftyp, moov (mvhd, a track with its own udta, a udta and a
/// meta), and an mdat.
fn movie(brand: &[u8; 4], handler: &[u8; 4], matrix: [u32; 9]) -> Vec<u8> {
    let ftyp = bx(
        b"ftyp",
        &[brand.as_slice(), &[0, 0, 2, 0], b"isom"].concat(),
    );
    let mdia = bx(b"mdia", &[mdhd(), hdlr(handler)].concat());
    let trak = bx(
        b"trak",
        &[
            tkhd(matrix),
            mdia,
            bx(b"udta", &bx(b"\xA9xyz", b"+48.8584+002.2945/")),
        ]
        .concat(),
    );
    let moov = bx(
        b"moov",
        &[
            mvhd(),
            trak,
            bx(b"udta", &bx(b"\xA9too", b"SECRET-SOFTWARE")),
            full(b"meta", 0, &bx(b"ilst", b"SECRET-ILST")),
        ]
        .concat(),
    );
    let mdat = bx(b"mdat", b"FRAMES-STAY-WHERE-THEY-ARE");
    [ftyp, moov, mdat].concat()
}

fn find(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap()
}

fn noise(w: u32, h: u32, seed: u32) -> RgbaImage {
    let mut state = seed | 1;
    RgbaImage::from_fn(w, h, |_, _| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let [r, g, b, _] = state.to_le_bytes();
        Rgba([r, g, b, 255])
    })
}

fn decoded(bytes: &[u8]) -> image::DynamicImage {
    image::load_from_memory(bytes).unwrap()
}

// ---------------------------------------------------------------------------
// Blobs

#[test]
fn seal_and_open_round_trip() {
    let sealed = seal(b"a picture").unwrap();
    assert_eq!(sealed.ciphertext.len(), 9 + TAG_BYTES);
    assert_eq!(sealed_len(9), sealed.ciphertext.len() as u64);
    assert_eq!(&sealed.hash, blake3::hash(b"a picture").as_bytes());
    let opened = open(&sealed.ciphertext, &sealed.key, &sealed.nonce, &sealed.hash).unwrap();
    assert_eq!(opened, b"a picture");
    // Every blob has its own key and nonce.
    let again = seal(b"a picture").unwrap();
    assert_ne!(again.key, sealed.key);
    assert_ne!(again.nonce, sealed.nonce);
}

#[test]
fn open_refuses_a_wrong_hash_or_tampered_bytes() {
    let sealed = seal(b"a picture").unwrap();
    let mut hash = sealed.hash;
    hash[0] ^= 1;
    assert_eq!(
        open(&sealed.ciphertext, &sealed.key, &sealed.nonce, &hash),
        Err(MediaError::HashMismatch)
    );
    let mut ciphertext = sealed.ciphertext.clone();
    ciphertext[0] ^= 1;
    assert_eq!(
        open(&ciphertext, &sealed.key, &sealed.nonce, &sealed.hash),
        Err(MediaError::Decrypt)
    );
    assert_eq!(
        open(
            &sealed.ciphertext,
            &sealed.key[..31],
            &sealed.nonce,
            &sealed.hash
        ),
        Err(MediaError::Malformed)
    );
    assert_eq!(
        open(
            &sealed.ciphertext,
            &sealed.key,
            &sealed.nonce,
            &sealed.hash[..31]
        ),
        Err(MediaError::Malformed)
    );
}

// ---------------------------------------------------------------------------
// Detection and bounds

#[test]
fn detects_formats_by_their_bytes() {
    assert_eq!(detect(&jpeg(4, 4)), Format::Jpeg);
    assert_eq!(detect(&png_with_text()), Format::Png);
    assert_eq!(detect(&gif(1)), Format::Gif);
    assert_eq!(detect(&movie(b"isom", b"vide", IDENTITY)), Format::Mp4);
    assert_eq!(detect(&movie(b"qt  ", b"vide", IDENTITY)), Format::Mov);
    assert_eq!(detect(&movie(b"M4A ", b"soun", IDENTITY)), Format::M4a);
    assert_eq!(detect(b"ID3\x04\0\0\0\0\0\0"), Format::Mp3);
    assert_eq!(detect(&[0xFF, 0xFB, 0x90, 0x64]), Format::Mp3);
    assert_eq!(detect(&[0xFF, 0xF1, 0x50, 0x80]), Format::Aac);
    assert_eq!(detect(b"OggS\0\x02"), Format::Ogg);
    assert_eq!(detect(b"fLaC\0\0\0\x22"), Format::Flac);
    assert_eq!(detect(b"RIFF\x24\0\0\0WAVEfmt "), Format::Wav);
    assert_eq!(
        detect(b"\x1A\x45\xDF\xA3\x9F\x42\x86\x81\x01\x42\x82\x84webm"),
        Format::WebM
    );
    assert_eq!(
        detect(b"\x1A\x45\xDF\xA3\x9F\x42\x86\x81\x01\x42\x82\x88matroska"),
        Format::Unknown
    );
    assert_eq!(detect(b"%PDF-1.7"), Format::Unknown);
    // The name says nothing.
    let processed = process(input(b"%PDF-1.7 ...".to_vec(), "photo.jpg")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::File);
    assert_eq!(processed.media_type, "application/octet-stream");
}

#[test]
fn probes_sizes_from_headers() {
    let p = probe(&jpeg(40, 30)).unwrap();
    assert_eq!((p.format, p.width, p.height), (Format::Jpeg, 40, 30));
    let p = probe(&gif(2)).unwrap();
    assert_eq!((p.format, p.width, p.height), (Format::Gif, 8, 6));
    let p = probe(&png(&RgbaImage::new(3, 5))).unwrap();
    assert_eq!((p.width, p.height), (3, 5));
    let p = probe(&webp_lossless(16, 8)).unwrap();
    assert_eq!((p.format, p.width, p.height), (Format::WebP, 16, 8));
    let p = probe(&heic()).unwrap();
    assert_eq!((p.format, p.width, p.height), (Format::Heic, 4032, 3024));
    assert_eq!(probe(b"not a picture"), None);
    assert_eq!(probe(&movie(b"isom", b"vide", IDENTITY)), None);
}

#[test]
fn bounds() {
    assert!(within_bounds(8192, 4882));
    assert!(!within_bounds(8192, 4883)); // over 40 million pixels
    assert!(!within_bounds(8193, 10));
    assert!(!within_bounds(0, 10));
}

#[test]
fn a_picture_past_the_bounds_goes_as_a_file() {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = 100_000u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&100_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes.extend(png_chunk(b"IHDR", &ihdr));
    bytes.extend(png_chunk(
        b"IDAT",
        &[0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
    ));
    bytes.extend(png_chunk(b"IEND", &[]));
    let p = probe(&bytes).unwrap();
    assert_eq!((p.width, p.height), (100_000, 100_000));
    assert!(!p.within_bounds());
    let processed = process(input(bytes.clone(), "huge.png")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::File);
    assert_eq!(processed.media_type, "image/png");
    assert_eq!(processed.main, bytes);
    assert_eq!(processed.thumbnail, None);
}

#[test]
fn a_picture_that_will_not_decode_goes_as_a_file() {
    let bytes = b"\xFF\xD8\xFF\xE0\x00\x02garbage".to_vec();
    let processed = process(input(bytes.clone(), "broken.jpg")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::File);
    assert_eq!(processed.media_type, "image/jpeg");
    assert_eq!(processed.main, bytes);
}

#[test]
fn a_jpeg_cut_off_in_its_scan_still_loses_its_metadata() {
    let mut bytes = camera_jpeg(16, 16);
    let scan = find(&bytes, &[0xFF, 0xDA]);
    bytes.truncate(scan + 20);
    let processed = process(Input {
        as_file: true,
        ..input(bytes.clone(), "cut.jpg")
    })
    .unwrap();
    assert!(!contains(&processed.main, b"SECRET"));
    assert!(contains(&processed.main, b"Exif\0\0"));
    assert!(processed.main.ends_with(&bytes[scan..]));
}

// ---------------------------------------------------------------------------
// Still images

#[test]
fn a_camera_jpeg_is_turned_upright_and_loses_its_metadata() {
    let processed = process(input(camera_jpeg(64, 32), "IMG_0001.JPG")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::Image);
    assert_eq!(processed.media_type, "image/jpeg");
    assert_eq!(processed.name, "IMG_0001.jpg");
    // Orientation 6: stored on its side, shown a quarter turn clockwise.
    assert_eq!((processed.width, processed.height), (32, 64));
    for needle in [&b"SECRET"[..], b"Exif", b"ICC_PROFILE"] {
        assert!(!contains(&processed.main, needle));
    }
    assert!(jpeg_markers(&processed.main)
        .iter()
        .all(|(m, _)| !matches!(m, 0xE1..=0xEF | 0xFE)));
    let shown = decoded(&processed.main).to_rgb8();
    assert_eq!(shown.dimensions(), (32, 64));
    // The left half (red) is now the top.
    let top = shown.get_pixel(16, 8).0;
    let bottom = shown.get_pixel(16, 56).0;
    assert!(top[0] > 200 && top[2] < 60, "{top:?}");
    assert!(bottom[2] > 200 && bottom[0] < 60, "{bottom:?}");

    let thumbnail = processed.thumbnail.unwrap();
    assert_eq!(decoded(&thumbnail).width(), 32);
    assert!(!processed.placeholder.is_empty());
    assert!(processed.placeholder.len() < 40);
}

#[test]
fn a_jpeg_as_a_file_keeps_its_pixels_and_only_its_orientation() {
    let original = camera_jpeg(64, 32);
    let processed = process(Input {
        as_file: true,
        ..input(original.clone(), "IMG_0001.JPG")
    })
    .unwrap();
    assert_eq!(processed.kind, AttachmentKind::File);
    assert_eq!(processed.media_type, "image/jpeg");
    assert_eq!(processed.name, "IMG_0001.JPG");
    assert!(!contains(&processed.main, b"SECRET"));
    let markers = jpeg_markers(&processed.main);
    let app1: Vec<_> = markers.iter().filter(|(m, _)| *m == 0xE1).collect();
    assert_eq!(app1.len(), 1);
    let exif = &app1[0].1;
    assert!(exif.starts_with(b"Exif\0\0"));
    assert_eq!(strip::exif_orientation(&exif[6..]), Some(6));
    // One entry, no GPS pointer.
    assert_eq!(&exif[14..16], &1u16.to_be_bytes());
    assert!(!contains(exif, &0x8825u16.to_be_bytes()));
    // The ICC profile stays.
    assert!(markers
        .iter()
        .any(|(m, body)| *m == 0xE2 && body.starts_with(b"ICC_PROFILE\0")));
    // The scan is untouched, and nothing follows EOI.
    let scan = find(&original, &[0xFF, 0xDA]);
    let end = find(&original, b"SECRET-TRAILER");
    assert!(processed.main.ends_with(&original[scan..end]));
    // Still a JPEG, still upright the same way.
    let (decoded_w, decoded_h) = (
        decoded(&processed.main).width(),
        decoded(&processed.main).height(),
    );
    assert_eq!((decoded_w, decoded_h), (64, 32));
}

#[test]
fn a_png_without_transparency_becomes_a_jpeg_and_loses_its_text() {
    let processed = process(input(png_with_text(), "shot.png")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::Image);
    assert_eq!(processed.media_type, "image/jpeg");
    assert_eq!(processed.name, "shot.jpg");
    assert!(!contains(&processed.main, b"SECRET"));
}

#[test]
fn a_png_as_a_file_loses_its_text_and_time() {
    let original = png_with_text();
    let processed = process(Input {
        as_file: true,
        ..input(original.clone(), "shot.png")
    })
    .unwrap();
    assert_eq!(processed.kind, AttachmentKind::File);
    assert!(!contains(&processed.main, b"tEXt"));
    assert!(!contains(&processed.main, b"tIME"));
    assert!(!contains(&processed.main, b"SECRET"));
    assert_eq!(decoded(&processed.main), decoded(&original));
}

#[test]
fn a_transparent_picture_stays_png() {
    let mut img = RgbaImage::from_pixel(30, 20, Rgba([0, 0, 255, 255]));
    img.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
    let processed = process(input(png(&img), "logo.png")).unwrap();
    assert_eq!(processed.media_type, "image/png");
    assert_eq!(processed.name, "logo.png");
    assert_eq!(decoded(&processed.main).to_rgba8(), img);
    let thumbnail = processed.thumbnail.unwrap();
    assert!(thumbnail.starts_with(b"\x89PNG"));
}

#[test]
fn a_webp_is_decoded_and_sent_as_jpeg() {
    let processed = process(input(webp_lossless(16, 8), "pic.webp")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::Image);
    assert_eq!(processed.media_type, "image/jpeg");
    assert_eq!((processed.width, processed.height), (16, 8));
}

#[test]
fn platform_pixels_make_the_image() {
    let mut rgba = vec![200u8; 10 * 10 * 4];
    rgba[3] = 0;
    let processed = process(Input {
        pixels: Some(Pixels {
            rgba,
            width: 10,
            height: 10,
        }),
        ..input(heic(), "IMG_0002.HEIC")
    })
    .unwrap();
    assert_eq!(processed.kind, AttachmentKind::Image);
    assert_eq!(processed.media_type, "image/png");
    assert_eq!(processed.name, "IMG_0002.png");
    assert_eq!((processed.width, processed.height), (10, 10));

    let wrong = process(Input {
        pixels: Some(Pixels {
            rgba: vec![0; 10],
            width: 10,
            height: 10,
        }),
        ..input(heic(), "IMG_0002.HEIC")
    });
    assert_eq!(wrong, Err(MediaError::Pixels));
}

#[test]
fn a_large_picture_shrinks_until_it_fits() {
    // A cap the picture fits at three quarters of its size and quality 55,
    // and at no quality at full size.
    let picture = noise(1000, 750, 7);
    let smaller = image::imageops::resize(
        &image::DynamicImage::ImageRgba8(picture.clone()).to_rgb8(),
        750,
        562,
        image::imageops::FilterType::CatmullRom,
    );
    let mut at_three_quarters = Vec::new();
    JpegEncoder::new_with_quality(&mut at_three_quarters, 55)
        .encode_image(&smaller)
        .unwrap();
    let cap = sealed_len(at_three_quarters.len());
    let processed = process(Input {
        caps: Caps {
            image_bytes: cap,
            ..Caps::default()
        },
        ..input(png(&picture), "noise.png")
    })
    .unwrap();
    assert_eq!(processed.main, at_three_quarters);
    assert_eq!((processed.width, processed.height), (750, 562));
    let shown = decoded(&processed.main);
    assert_eq!((shown.width(), shown.height()), (750, 562));
}

#[test]
fn a_picture_that_cannot_fit_at_640_is_refused() {
    let result = process(Input {
        caps: Caps {
            image_bytes: 8 * 1024,
            ..Caps::default()
        },
        ..input(png(&noise(700, 700, 3)), "noise.png")
    });
    assert_eq!(result, Err(MediaError::ImageTooLarge { cap: 8 * 1024 }));
}

/// The full fitting loop at camera size. Slow in a debug build (the JPEG
/// encoder runs unoptimised), so it runs with `--release` or `--ignored`.
#[test]
#[cfg_attr(debug_assertions, ignore)]
fn a_noisy_camera_picture_fits_two_mebibytes() {
    let processed = process(input(png(&noise(4000, 3000, 11)), "noise.png")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::Image);
    assert!(sealed_len(processed.main.len()) <= Caps::default().image_bytes);
    assert!(processed.width <= 2560 && processed.width >= 640);
    assert!((processed.width * 3).abs_diff(processed.height * 4) <= 4);
    assert!(sealed_len(processed.thumbnail.unwrap().len()) <= THUMBNAIL_MAX_BYTES);
}

// ---------------------------------------------------------------------------
// GIFs

#[test]
fn an_animated_gif_is_kept_but_its_comments() {
    let (original, inserted) = gif_with_comment(2);
    let processed = process(input(original.clone(), "dance.gif")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::Gif);
    assert_eq!(processed.media_type, "image/gif");
    assert_eq!((processed.width, processed.height), (8, 6));
    assert!(!contains(&processed.main, b"SECRET"));
    assert!(contains(&processed.main, b"NETSCAPE2.0"));
    assert_eq!(processed.main.len(), original.len() - inserted);
    assert_eq!(processed.main, gif(2));
    let thumbnail = decoded(&processed.thumbnail.unwrap());
    assert_eq!((thumbnail.width(), thumbnail.height()), (8, 6));
    assert!(!processed.placeholder.is_empty());
}

#[test]
fn a_gif_of_one_frame_is_an_image() {
    let (original, _) = gif_with_comment(1);
    let processed = process(input(original, "still.gif")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::Image);
    assert_eq!(processed.name, "still.jpg");
}

// ---------------------------------------------------------------------------
// Video and audio

#[test]
fn a_movie_loses_its_metadata_in_place() {
    let original = movie(b"isom", b"vide", IDENTITY);
    let processed = process(input(original.clone(), "clip.mp4")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::Video);
    assert_eq!(processed.media_type, "video/mp4");
    assert_eq!((processed.width, processed.height), (1920, 1080));
    assert_eq!(processed.duration_ms, 5000);
    assert_eq!(processed.thumbnail, None);
    let out = &processed.main;
    assert_eq!(out.len(), original.len());
    for needle in [&b"SECRET"[..], b"+48.8584", b"udta", b"meta", b"ilst"] {
        assert!(
            !contains(out, needle),
            "{}",
            String::from_utf8_lossy(needle)
        );
    }
    // Every udta and meta is a free box of its size, zeroed.
    for kind in [&b"udta"[..], b"meta"] {
        let mut from = 0;
        while let Some(at) = original[from..].windows(4).position(|w| w == kind) {
            let at = from + at;
            let size =
                usize::try_from(u32::from_be_bytes(original[at - 4..at].try_into().unwrap()))
                    .unwrap();
            assert_eq!(&out[at - 4..at], &original[at - 4..at]);
            assert_eq!(&out[at..at + 4], b"free");
            assert!(out[at + 4..at - 4 + size].iter().all(|&b| b == 0));
            from = at + 4;
        }
    }
    // Times zeroed, the rest of each header kept.
    let mvhd = find(out, b"mvhd") + 8;
    assert_eq!(&out[mvhd..mvhd + 8], &[0; 8]);
    assert_eq!(&out[mvhd + 8..mvhd + 12], &600u32.to_be_bytes());
    let tkhd = find(out, b"tkhd") + 8;
    assert_eq!(&out[tkhd..tkhd + 16], &[0; 16]);
    assert_eq!(&out[tkhd + 16..tkhd + 20], &1u32.to_be_bytes());
    let mdhd = find(out, b"mdhd") + 8;
    assert_eq!(&out[mdhd..mdhd + 8], &[0; 8]);
    // The media data has not moved.
    let mdat = find(&original, b"FRAMES");
    assert_eq!(&out[mdat..mdat + 6], b"FRAMES");
}

#[test]
fn a_turned_movie_reports_its_upright_size_and_its_poster() {
    let processed = process(Input {
        poster: Some(Pixels {
            rgba: noise(108, 192, 5).into_raw(),
            width: 108,
            height: 192,
        }),
        ..input(movie(b"qt  ", b"vide", QUARTER), "IMG_0003.MOV")
    })
    .unwrap();
    assert_eq!(processed.media_type, "video/quicktime");
    assert_eq!((processed.width, processed.height), (1080, 1920));
    let thumbnail = decoded(&processed.thumbnail.unwrap());
    assert_eq!((thumbnail.width(), thumbnail.height()), (108, 192));
    assert!(!processed.placeholder.is_empty());
}

#[test]
fn a_movie_as_a_file_is_wiped_too() {
    let processed = process(Input {
        as_file: true,
        ..input(movie(b"isom", b"vide", IDENTITY), "clip.mp4")
    })
    .unwrap();
    assert_eq!(processed.kind, AttachmentKind::File);
    assert_eq!(processed.media_type, "video/mp4");
    assert!(!contains(&processed.main, b"SECRET"));
}

#[test]
fn m4a_is_audio_without_its_tags() {
    let processed = process(input(movie(b"M4A ", b"soun", IDENTITY), "song.m4a")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::Audio);
    assert_eq!(processed.media_type, "audio/mp4");
    assert_eq!(processed.duration_ms, 5000);
    assert!(!contains(&processed.main, b"SECRET"));
    assert_eq!(processed.thumbnail, None);
}

#[test]
fn other_audio_and_webm_go_as_chosen() {
    let mp3 = b"ID3\x04\0\0\0\0\0\0\xFF\xFB\x90\x64".to_vec();
    let processed = process(Input {
        duration_ms: Some(1234),
        ..input(mp3.clone(), "song.mp3")
    })
    .unwrap();
    assert_eq!(processed.kind, AttachmentKind::Audio);
    assert_eq!(processed.media_type, "audio/mpeg");
    assert_eq!(processed.duration_ms, 1234);
    assert_eq!(processed.main, mp3);

    let webm = b"\x1A\x45\xDF\xA3\x9F\x42\x86\x81\x01\x42\x82\x84webm\0\0".to_vec();
    let processed = process(Input {
        duration_ms: Some(9000),
        video_size: Some((640, 360)),
        ..input(webm.clone(), "clip.webm")
    })
    .unwrap();
    assert_eq!(processed.kind, AttachmentKind::Video);
    assert_eq!(processed.media_type, "video/webm");
    assert_eq!(
        (processed.width, processed.height, processed.duration_ms),
        (640, 360, 9000)
    );
    assert_eq!(processed.main, webm);
}

// ---------------------------------------------------------------------------
// Files

fn webp_lossless(w: u32, h: u32) -> Vec<u8> {
    let img = RgbaImage::from_pixel(w, h, Rgba([20, 120, 220, 255]));
    let mut out = Vec::new();
    WebPEncoder::new_lossless(Cursor::new(&mut out))
        .write_image(img.as_raw(), w, h, ExtendedColorType::Rgba8)
        .unwrap();
    out
}

fn riff_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut c = kind.to_vec();
    c.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
    c.extend_from_slice(data);
    if data.len() % 2 == 1 {
        c.push(0);
    }
    c
}

#[test]
fn a_webp_as_a_file_loses_exif_and_xmp() {
    let mut vp8x = vec![0x20 | 0x08 | 0x04, 0, 0, 0];
    vp8x.extend_from_slice(&[15, 0, 0, 7, 0, 0]); // 16 x 8
    let chunks = [
        riff_chunk(b"VP8X", &vp8x),
        riff_chunk(b"ICCP", b"profile"),
        riff_chunk(b"VP8L", b"\x2F pixels"),
        riff_chunk(b"EXIF", b"SECRET-EXIF"),
        riff_chunk(b"XMP ", b"SECRET-XMP"),
    ]
    .concat();
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&u32::try_from(chunks.len() + 4).unwrap().to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(&chunks);
    let p = probe(&bytes).unwrap();
    assert_eq!((p.width, p.height), (16, 8));

    let processed = process(Input {
        as_file: true,
        ..input(bytes, "pic.webp")
    })
    .unwrap();
    let out = &processed.main;
    assert_eq!(processed.media_type, "image/webp");
    assert!(!contains(out, b"SECRET"));
    assert!(!contains(out, b"EXIF"));
    assert_eq!(out[20], 0x20);
    assert_eq!(
        u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize,
        out.len() - 8
    );
    assert!(contains(out, b"ICCP\x07\0\0\0profile"));
    assert!(contains(out, b"\x2F pixels"));
}

/// A HEIC: ftyp, a meta box with an image item and an Exif item located
/// by iloc in the mdat, and an ispe.
fn heic() -> Vec<u8> {
    let ftyp = bx(b"ftyp", b"heic\0\0\0\0mif1heic");
    let build = |pixels_at: u32, exif_at: u32| {
        let infe = |id: u16, kind: &[u8; 4]| {
            full(
                b"infe",
                2,
                &[&id.to_be_bytes()[..], &[0, 0], kind, b"\0"].concat(),
            )
        };
        let iinf = full(
            b"iinf",
            0,
            &[
                &2u16.to_be_bytes()[..],
                &infe(1, b"hvc1"),
                &infe(2, b"Exif"),
            ]
            .concat(),
        );
        let mut iloc = vec![0x44, 0x00]; // offset 4, length 4, base 0
        iloc.extend_from_slice(&2u16.to_be_bytes());
        for (id, at, len) in [(1u16, pixels_at, 6u32), (2, exif_at, 11)] {
            iloc.extend_from_slice(&id.to_be_bytes());
            iloc.extend_from_slice(&0u16.to_be_bytes()); // data reference
            iloc.extend_from_slice(&1u16.to_be_bytes()); // one extent
            iloc.extend_from_slice(&at.to_be_bytes());
            iloc.extend_from_slice(&len.to_be_bytes());
        }
        let ispe = |w: u32, h: u32| full(b"ispe", 0, &[w.to_be_bytes(), h.to_be_bytes()].concat());
        let iprp = bx(
            b"iprp",
            &bx(b"ipco", &[ispe(320, 240), ispe(4032, 3024)].concat()),
        );
        full(
            b"meta",
            0,
            &[hdlr(b"pict"), iinf, full(b"iloc", 0, &iloc), iprp].concat(),
        )
    };
    let meta_len = build(0, 0).len();
    let data = ftyp.len() + meta_len + 8;
    let meta = build(
        u32::try_from(data).unwrap(),
        u32::try_from(data + 6).unwrap(),
    );
    let mdat = bx(b"mdat", b"PIXELSSECRET-EXIF");
    [ftyp, meta, mdat].concat()
}

#[test]
fn a_heic_as_a_file_loses_its_exif_in_place() {
    let original = heic();
    let processed = process(input(original.clone(), "IMG_0004.HEIC")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::File);
    assert_eq!(processed.media_type, "image/heic");
    let out = &processed.main;
    assert_eq!(out.len(), original.len());
    assert!(contains(out, b"PIXELS"));
    assert!(!contains(out, b"SECRET"));
    let at = find(&original, b"SECRET-EXIF");
    assert!(out[at..at + 11].iter().all(|&b| b == 0));
}

#[test]
fn other_files_go_as_chosen_with_a_type_from_their_name() {
    let processed = process(input(b"%PDF-1.7 author: me".to_vec(), "report.PDF")).unwrap();
    assert_eq!(processed.kind, AttachmentKind::File);
    assert_eq!(processed.media_type, "application/pdf");
    assert_eq!(processed.main, b"%PDF-1.7 author: me");
    let processed = process(input(vec![1, 2, 3], "blob")).unwrap();
    assert_eq!(processed.media_type, "application/octet-stream");
}

#[test]
fn a_file_over_the_cap_is_refused() {
    let result = process(Input {
        caps: Caps {
            file_bytes: 50,
            ..Caps::default()
        },
        ..input(vec![7; 40], "big.bin")
    });
    assert_eq!(result, Err(MediaError::FileTooLarge { size: 56, cap: 50 }));
}

#[test]
fn names_are_cut_on_a_character_boundary() {
    let long = "é".repeat(200);
    let clipped = clip_name(&long);
    assert_eq!(clipped.len(), 254);
    assert!(clipped.chars().all(|c| c == 'é'));
    let processed = process(input(png_with_text(), &format!("{long}.png"))).unwrap();
    assert!(processed.name.len() <= NAME_MAX_BYTES);
    assert!(processed.name.ends_with(".jpg"));
}

#[test]
fn a_placeholder_opens_as_a_small_picture_of_the_same_shape() {
    let processed = process(input(jpeg(800, 400), "wide.jpg")).unwrap();
    let small = placeholder_image(&processed.placeholder).unwrap();
    assert_eq!(small.width, 32);
    assert!((14..=18).contains(&small.height));
    assert_eq!(small.rgba.len(), (small.width * small.height * 4) as usize);
    assert!(placeholder_image(&[]).is_none());
    assert!(placeholder_image(&[1, 2, 3]).is_none());
}
