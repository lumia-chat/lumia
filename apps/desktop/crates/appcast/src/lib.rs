//! The update feed off macOS: a Sparkle appcast, signed as Sparkle 2's `sign_update` signs
//! one, so the Linux updater and the release agree with the Mac app on format and key
//! (architecture section 13, "How the desktop shell updates itself").
//!
//! Two signatures, both Ed25519 with the release's key, both base64:
//!
//! - **The feed's.** Sparkle appends a comment after the document,
//!   `<!-- sparkle-signatures:\nedSignature: …\nlength: N\n-->`, whose signature covers the
//!   first N bytes, which are everything before the comment. [`read`] refuses a feed
//!   without it, as the Mac app does with `SURequireSignedFeed`: a download host that was
//!   taken over can then neither relabel an old build as the newest nor point at a file of
//!   its own.
//! - **Each download's.** The enclosure's `sparkle:edSignature`, over the file's bytes,
//!   with its `length` beside it ([`verify`]).
//!
//! A feed that verifies may still be an old one served again; [`newest_above`] only offers
//! a release whose version is above the running one, which is the downgrade check.
//!
//! The key file is what Sparkle's tools read: the 32-byte Ed25519 seed in base64. The
//! public half is the same base64 the Mac bundle carries as `SUPublicEDKey`.

use std::cmp::Ordering;
use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

/// Sparkle's XML namespace.
pub const SPARKLE_NS: &str = "http://www.andymatuschak.org/xml-namespaces/sparkle";
/// Where the feed's own signature starts, after the document.
const SIGNATURES: &str = "<!-- sparkle-signatures:\n";
/// The largest feed read: one release is well under a kilobyte.
pub const MAX_FEED: usize = 1 << 20;

/// Why a feed, a key or a download was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A key that is not 32 bytes of base64.
    Key,
    /// A feed with no signature of its own after the document.
    Unsigned,
    /// A signature that is not 64 bytes of base64, or does not verify with the key.
    Signature,
    /// A feed that is not the XML an appcast is, or a release in it missing what it needs.
    Malformed(String),
    /// A download whose length is not the one the feed gives.
    Length { expected: u64, found: u64 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key => f.write_str("not an Ed25519 key (32 bytes, base64)"),
            Self::Unsigned => f.write_str("the feed carries no signature of its own"),
            Self::Signature => f.write_str("the signature does not verify with the release key"),
            Self::Malformed(why) => write!(f, "not an update feed: {why}"),
            Self::Length { expected, found } => {
                write!(
                    f,
                    "the download is {found} bytes, not the {expected} the feed gives"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// The public half of the release key, which the shell is built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicKey(VerifyingKey);

impl PublicKey {
    /// From the base64 the Mac bundle carries as `SUPublicEDKey`.
    pub fn from_base64(text: &str) -> Result<Self, Error> {
        let bytes: [u8; 32] = decode(text)?.try_into().map_err(|_| Error::Key)?;
        VerifyingKey::from_bytes(&bytes)
            .map(Self)
            .map_err(|_| Error::Key)
    }

    pub fn to_base64(self) -> String {
        STANDARD.encode(self.0.as_bytes())
    }
}

/// The private half, which only the release holds.
pub struct PrivateKey(SigningKey);

impl PrivateKey {
    /// From a key file's contents: the 32-byte seed in base64, as Sparkle's tools read it.
    pub fn from_base64(text: &str) -> Result<Self, Error> {
        let bytes: [u8; 32] = decode(text)?.try_into().map_err(|_| Error::Key)?;
        Ok(Self(SigningKey::from_bytes(&bytes)))
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.verifying_key())
    }

    /// The signature of `bytes`, in base64: an enclosure's `sparkle:edSignature`.
    pub fn sign(&self, bytes: &[u8]) -> String {
        STANDARD.encode(self.0.sign(bytes).to_bytes())
    }

    /// `feed` with its own signature after it, as `sign_update` leaves a feed.
    pub fn sign_feed(&self, feed: &str) -> String {
        format!(
            "{feed}{SIGNATURES}edSignature: {}\nlength: {}\n-->\n",
            self.sign(feed.as_bytes()),
            feed.len()
        )
    }
}

fn decode(text: &str) -> Result<Vec<u8>, Error> {
    STANDARD.decode(text.trim()).map_err(|_| Error::Key)
}

/// Whether `signature` (base64) is the key's over `bytes`.
pub fn verify(key: PublicKey, bytes: &[u8], signature: &str) -> Result<(), Error> {
    let raw = STANDARD
        .decode(signature.trim())
        .map_err(|_| Error::Signature)?;
    let signature = Signature::from_slice(&raw).map_err(|_| Error::Signature)?;
    key.0
        .verify_strict(bytes, &signature)
        .map_err(|_| Error::Signature)
}

/// The part of `feed` its own signature covers, once that signature verifies: everything
/// before the signatures comment, which must start exactly where its `length` says.
pub fn signed_part(feed: &[u8], key: PublicKey) -> Result<&str, Error> {
    if feed.len() > MAX_FEED {
        return Err(Error::Malformed("larger than any feed".into()));
    }
    let text = std::str::from_utf8(feed).map_err(|_| Error::Malformed("not UTF-8".into()))?;
    let at = text.rfind(SIGNATURES).ok_or(Error::Unsigned)?;
    let tail = text[at + SIGNATURES.len()..].trim_end();
    let fields = tail.strip_suffix("-->").ok_or(Error::Unsigned)?;
    let (mut signature, mut length) = (None, None);
    for line in fields
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match line.split_once(':') {
            Some(("edSignature", value)) if signature.is_none() => signature = Some(value.trim()),
            Some(("length", value)) if length.is_none() => length = Some(value.trim()),
            _ => return Err(Error::Unsigned),
        }
    }
    let (Some(signature), Some(length)) = (signature, length) else {
        return Err(Error::Unsigned);
    };
    if length.parse::<usize>().ok() != Some(at) {
        return Err(Error::Signature);
    }
    let signed = &text[..at];
    verify(key, signed.as_bytes(), signature)?;
    Ok(signed)
}

/// One release in a feed: what to download, how to check it, and its version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// `sparkle:version`, which is what is compared.
    pub version: String,
    /// `sparkle:shortVersionString`, what a person is shown; the version when absent.
    pub display: String,
    pub url: String,
    pub length: u64,
    /// The download's `sparkle:edSignature`.
    pub signature: String,
}

impl Release {
    /// Whether `download` is this release's file: its length, then its signature.
    pub fn check(&self, download: &[u8], key: PublicKey) -> Result<(), Error> {
        let found = download.len() as u64;
        if found != self.length {
            return Err(Error::Length {
                expected: self.length,
                found,
            });
        }
        verify(key, download, &self.signature)
    }
}

/// The releases in `feed`, once its own signature verifies with `key`. An item without a
/// version, an `https` download, a length or a signature is refused with the feed: a feed
/// this key signed is the release's own, and one it got wrong is not guessed at.
pub fn read(feed: &[u8], key: PublicKey) -> Result<Vec<Release>, Error> {
    let signed = signed_part(feed, key)?;
    let doc = roxmltree::Document::parse(signed).map_err(|e| Error::Malformed(e.to_string()))?;
    let channel = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("channel"))
        .ok_or_else(|| Error::Malformed("no channel".into()))?;
    channel
        .children()
        .filter(|n| n.has_tag_name("item"))
        .map(|item| release(item))
        .collect()
}

fn release(item: roxmltree::Node<'_, '_>) -> Result<Release, Error> {
    let sparkle = |name: &str| {
        item.children()
            .find(|n| n.has_tag_name((SPARKLE_NS, name)))
            .and_then(|n| n.text())
            .map(|text| text.trim().to_string())
    };
    let enclosure = item
        .children()
        .find(|n| n.has_tag_name("enclosure"))
        .ok_or_else(|| Error::Malformed("an item has no enclosure".into()))?;
    let attribute = |name: &str| enclosure.attribute(name).map(str::to_string);
    let version = sparkle("version")
        .or_else(|| {
            enclosure
                .attribute((SPARKLE_NS, "version"))
                .map(str::to_string)
        })
        .filter(|v| !v.is_empty())
        .ok_or_else(|| Error::Malformed("an item has no version".into()))?;
    let url = attribute("url")
        .filter(|url| url.starts_with("https://"))
        .ok_or_else(|| Error::Malformed("an item's download is not https".into()))?;
    let length = attribute("length")
        .and_then(|length| length.parse().ok())
        .ok_or_else(|| Error::Malformed("an item's download has no length".into()))?;
    let signature = enclosure
        .attribute((SPARKLE_NS, "edSignature"))
        .map(str::to_string)
        .ok_or_else(|| Error::Malformed("an item's download is not signed".into()))?;
    Ok(Release {
        display: sparkle("shortVersionString").unwrap_or_else(|| version.clone()),
        version,
        url,
        length,
        signature,
    })
}

/// The release with the highest version above `current`, if there is one. A version that
/// is not dotted numbers (as every Lumia version is) is never offered.
pub fn newest_above<'a>(releases: &'a [Release], current: &str) -> Option<&'a Release> {
    releases
        .iter()
        .filter(|r| compare(&r.version, current) == Some(Ordering::Greater))
        .max_by(|a, b| compare(&a.version, &b.version).unwrap_or(Ordering::Equal))
}

/// Compares two versions of dotted numbers, a missing part being 0 (`0.10` is above
/// `0.9.0`, and `1.0` is `1.0.0`). None when either is anything else.
pub fn compare(a: &str, b: &str) -> Option<Ordering> {
    let parts = |v: &str| -> Option<Vec<u64>> {
        v.split('.')
            .map(|part| {
                (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| part.parse().ok())
                    .flatten()
            })
            .collect()
    };
    let (a, b) = (parts(a)?, parts(b)?);
    let width = a.len().max(b.len());
    let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    Some(
        (0..width)
            .map(|i| at(&a, i).cmp(&at(&b, i)))
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal),
    )
}

/// A feed of one release, unsigned: what [`PrivateKey::sign_feed`] then signs. `version` is
/// both the compared version and the one shown.
pub fn write(title: &str, version: &str, url: &str, length: u64, signature: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<rss xmlns:sparkle="{SPARKLE_NS}" version="2.0">
    <channel>
        <title>{title}</title>
        <item>
            <title>{version}</title>
            <sparkle:version>{version}</sparkle:version>
            <sparkle:shortVersionString>{version}</sparkle:shortVersionString>
            <enclosure url="{url}" length="{length}" type="application/octet-stream" sparkle:edSignature="{signature}"/>
        </item>
    </channel>
</rss>"#,
        title = escape(title),
        version = escape(version),
        url = escape(url),
        signature = escape(signature),
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
