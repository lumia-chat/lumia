//! The feed against Sparkle's own tools and against what a taken-over host could serve.
//!
//! `fixtures/sparkle-signed.xml` is a feed that Sparkle 2.10.1's `sign_update` signed on a
//! Mac with the throwaway key below, made only for this test; `HELLO` is that tool's
//! signature of the five bytes `hello` with the same key. Reading the one and checking the
//! other shows this crate and Sparkle agree on both signatures.

#![allow(clippy::unwrap_used)]

use std::cmp::Ordering;

use lumia_appcast::{Error, PrivateKey, PublicKey, compare, newest_above, read, signed_part};

/// A throwaway seed, used for nothing but these fixtures.
const SEED: &str = "NyS8HjwxfipXZ5AQLnqcLvSFNZGXrmIkCWLAWfrrx+w=";
const SPARKLE_FEED: &[u8] = include_bytes!("fixtures/sparkle-signed.xml");
const HELLO: &str =
    "OC8YqnUmhJ3fHGBHUZPuGggZphl4MufLDQik6TUE8XWuOOp2tW9rWWZPFr0+zfCICJcMXViDGZIhHxvvfEJxCA==";

fn key() -> PrivateKey {
    PrivateKey::from_base64(SEED).unwrap()
}

fn other_key() -> PrivateKey {
    PrivateKey::from_base64("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap()
}

/// A signed feed of one release, as the release writes it.
fn feed(version: &str, download: &[u8]) -> String {
    let key = key();
    let unsigned = lumia_appcast::write(
        "Lumia",
        version,
        &format!("https://example.com/Lumia-{version}.AppImage"),
        download.len() as u64,
        &key.sign(download),
    );
    key.sign_feed(&unsigned)
}

#[test]
fn sparkles_signed_feed_verifies() {
    let public = key().public();
    let signed = signed_part(SPARKLE_FEED, public).unwrap();
    assert!(signed.ends_with("</rss>"));
    // The fixture's one item carries a made-up download signature; the feed's own is real.
    let releases = read(SPARKLE_FEED, public).unwrap();
    assert_eq!(releases[0].version, "0.10.0");
    assert_eq!(releases[0].url, "https://x/Lumia.AppImage");
    assert_eq!(
        releases[0].check(b"hello", public).unwrap_err(),
        Error::Signature
    );
}

#[test]
fn signatures_match_sparkles() {
    let key = key();
    assert_eq!(key.sign(b"hello"), HELLO);
    lumia_appcast::verify(key.public(), b"hello", HELLO).unwrap();
    // Ed25519 is deterministic: signing what Sparkle signed gives Sparkle's signature, so
    // a feed this crate signs reads the same in Sparkle.
    let text = std::str::from_utf8(SPARKLE_FEED).unwrap();
    let at = text.rfind("<!-- sparkle-signatures:").unwrap();
    assert_eq!(key.sign_feed(&text[..at]), text);
}

#[test]
fn a_written_feed_reads_back() {
    let download = b"an AppImage";
    let releases = read(feed("0.10.0", download).as_bytes(), key().public()).unwrap();
    assert_eq!(releases.len(), 1);
    let release = &releases[0];
    assert_eq!(release.version, "0.10.0");
    assert_eq!(release.display, "0.10.0");
    assert_eq!(release.url, "https://example.com/Lumia-0.10.0.AppImage");
    release.check(download, key().public()).unwrap();
}

#[test]
fn a_feed_signed_with_another_key_is_refused() {
    let unsigned = lumia_appcast::write("Lumia", "9.0.0", "https://x/y", 1, "sig");
    let forged = other_key().sign_feed(&unsigned);
    assert_eq!(
        read(forged.as_bytes(), key().public()).unwrap_err(),
        Error::Signature
    );
}

#[test]
fn an_unsigned_feed_is_refused() {
    let unsigned = lumia_appcast::write("Lumia", "9.0.0", "https://x/y", 1, "sig");
    assert_eq!(
        read(unsigned.as_bytes(), key().public()).unwrap_err(),
        Error::Unsigned
    );
}

#[test]
fn a_relabelled_feed_is_refused() {
    // The host changes the version of a feed the release signed.
    let relabelled =
        feed("0.10.0", b"old").replace("<sparkle:version>0.10.0<", "<sparkle:version>9.0.0<");
    assert_eq!(
        read(relabelled.as_bytes(), key().public()).unwrap_err(),
        Error::Signature
    );
}

#[test]
fn a_signature_moved_off_the_end_is_refused() {
    // Something added between the document and its signature: the length no longer meets
    // the comment.
    let signed = feed("0.10.0", b"x");
    let at = signed.rfind("<!-- sparkle-signatures:").unwrap();
    let padded = format!("{}<!-- more -->{}", &signed[..at], &signed[at..]);
    assert_eq!(
        read(padded.as_bytes(), key().public()).unwrap_err(),
        Error::Signature
    );
}

#[test]
fn a_download_that_is_not_the_releases_is_refused() {
    let releases = read(feed("0.10.0", b"the real one").as_bytes(), key().public()).unwrap();
    let release = &releases[0];
    assert_eq!(
        release.check(b"the fake one", key().public()).unwrap_err(),
        Error::Signature
    );
    assert_eq!(
        release.check(b"short", key().public()).unwrap_err(),
        Error::Length {
            expected: 12,
            found: 5
        }
    );
}

#[test]
fn an_old_signed_feed_offers_nothing() {
    // A host serving an older release's feed, validly signed: nothing is above this one.
    let releases = read(feed("0.8.0", b"old").as_bytes(), key().public()).unwrap();
    assert_eq!(newest_above(&releases, "0.9.0"), None);
    let same = read(feed("0.9.0", b"same").as_bytes(), key().public()).unwrap();
    assert_eq!(newest_above(&same, "0.9.0"), None);
    let newer = read(feed("0.10.0", b"new").as_bytes(), key().public()).unwrap();
    assert_eq!(newest_above(&newer, "0.9.0").unwrap().version, "0.10.0");
}

#[test]
fn versions_compare_as_numbers() {
    assert_eq!(compare("0.10.0", "0.9.0"), Some(Ordering::Greater));
    assert_eq!(compare("1.0", "1.0.0"), Some(Ordering::Equal));
    assert_eq!(compare("0.9.1", "0.9.10"), Some(Ordering::Less));
    assert_eq!(compare("1.0.0-rc1", "0.9.0"), None);
    assert_eq!(compare("", "0.9.0"), None);
    assert_eq!(compare("1..0", "0.9.0"), None);
}

#[test]
fn keys_round_trip() {
    let public = key().public();
    assert_eq!(PublicKey::from_base64(&public.to_base64()).unwrap(), public);
    assert_eq!(PublicKey::from_base64("c2hvcnQ=").unwrap_err(), Error::Key);
    assert!(PrivateKey::from_base64("not base64!").is_err());
}
