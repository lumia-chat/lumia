//! Cross-implementation embed vectors from `proto/vectors/embeds/cases.json` (media spec
//! 6.6): links and the embed each gives, or none. Every client that reads links into embeds
//! must agree on every vector.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lumia_core::embeds::{bluesky_embed_url, embed_for, Embed, Shape, Source};
use serde::Deserialize;

const PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../proto/vectors/embeds/cases.json"
);

#[derive(Deserialize)]
struct File {
    vectors: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    url: String,
    expect: Option<Expect>,
    why: String,
}

/// An embed as the vectors write it: `url`, or a Bluesky handle and record key to resolve;
/// `ratio` (width, height) for a player, or `height` (with `resizes` when the embed grows to
/// what it reports).
#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct Expect {
    provider: String,
    kind: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    bluesky_handle: Option<String>,
    #[serde(default)]
    bluesky_rkey: Option<String>,
    #[serde(default)]
    ratio: Option<[u32; 2]>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    resizes: bool,
    #[serde(default)]
    needs_parent: bool,
    label: String,
}

impl From<&Embed> for Expect {
    fn from(e: &Embed) -> Self {
        let (url, bluesky_handle, bluesky_rkey) = match &e.source {
            Source::Url(u) => (Some(u.clone()), None, None),
            Source::BlueskyHandle { handle, rkey } => {
                (None, Some(handle.clone()), Some(rkey.clone()))
            }
        };
        let (ratio, height, resizes) = match e.shape {
            Shape::Ratio { width, height } => (Some([width, height]), None, false),
            Shape::Fixed { height } => (None, Some(height), false),
            Shape::Resizes { height } => (None, Some(height), true),
        };
        Self {
            provider: e.provider.id().to_string(),
            kind: e.kind.id().to_string(),
            url,
            bluesky_handle,
            bluesky_rkey,
            ratio,
            height,
            resizes,
            needs_parent: e.needs_parent,
            label: e.label(),
        }
    }
}

fn cases() -> Vec<Case> {
    let text = std::fs::read_to_string(PATH).unwrap_or_else(|e| panic!("{PATH}: {e}"));
    let file: File = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{PATH}: {e}"));
    assert!(!file.vectors.is_empty(), "{PATH} has no vectors");
    file.vectors
}

#[test]
fn every_link_gives_its_embed() {
    let mut failed = Vec::new();
    for case in cases() {
        let got = embed_for(&case.url).as_ref().map(Expect::from);
        if got != case.expect {
            failed.push(format!(
                "{} ({}):\n  got  {got:?}\n  want {:?}",
                case.url, case.why, case.expect
            ));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn every_site_has_cases_both_ways() {
    let cases = cases();
    for provider in lumia_core::embeds::Provider::ALL {
        let hits = cases
            .iter()
            .filter(|c| {
                c.expect
                    .as_ref()
                    .is_some_and(|e| e.provider == provider.id())
            })
            .count();
        assert!(hits >= 3, "{} has {hits} embedding cases", provider.id());
    }
    assert!(cases.iter().filter(|c| c.expect.is_none()).count() >= 50);
}

#[test]
fn a_resolved_handle_gives_the_address_a_did_link_does() {
    let did = "did:plc:z72i7hdynmk6r22z27h6tvur";
    let rkey = "3k2la3rsg5w2y";
    let by_did = embed_for(&format!("https://bsky.app/profile/{did}/post/{rkey}")).unwrap();
    assert_eq!(
        by_did.source,
        Source::Url(bluesky_embed_url(did, rkey).unwrap())
    );
    assert_eq!(bluesky_embed_url("did:plc:nope", rkey), None);
    assert_eq!(bluesky_embed_url(&format!("{did}/x"), rkey), None);
}
