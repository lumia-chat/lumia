//! Cross-implementation link card vectors from `proto/vectors/cards/` (media spec 6):
//! pages and the card each gives (`cases.json`), message bodies and the links that may
//! have cards (`links.json`), and addresses a fetcher may connect to (`addresses.json`).
//! The instance's card builder reads the same files; both must agree on every vector.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::IpAddr;

use lumia_core::cards::{address_allowed, card_links, parse_head, CardDraft};
use serde::Deserialize;

const DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../proto/vectors/cards"
);

#[derive(Deserialize)]
struct File<T> {
    vectors: Vec<T>,
}

fn read<T: for<'de> Deserialize<'de>>(name: &str) -> Vec<T> {
    let path = format!("{DIR}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let file: File<T> = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(!file.vectors.is_empty(), "{path} has no vectors");
    file.vectors
}

#[derive(Deserialize)]
struct Page {
    name: String,
    file: String,
    page_url: String,
    expect: Option<Card>,
}

#[derive(Deserialize)]
struct Card {
    url: String,
    title: String,
    description: String,
    site_name: String,
    color: String,
    image_url: String,
    image_width: u32,
    image_height: u32,
    video: bool,
}

impl From<Card> for CardDraft {
    fn from(c: Card) -> Self {
        CardDraft {
            url: c.url,
            title: c.title,
            description: c.description,
            site_name: c.site_name,
            color: c.color,
            image_url: c.image_url,
            image_width: c.image_width,
            image_height: c.image_height,
            video: c.video,
        }
    }
}

#[test]
fn every_page_gives_its_card() {
    let mut failed = Vec::new();
    for page in read::<Page>("cases.json") {
        let html = std::fs::read(format!("{DIR}/{}", page.file)).unwrap();
        let got = parse_head(&html, &page.page_url);
        let want = page.expect.map(CardDraft::from);
        if got != want {
            failed.push(format!("{}:\n  got  {got:?}\n  want {want:?}", page.name));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[derive(Deserialize)]
struct Links {
    name: String,
    body: String,
    links: Vec<String>,
}

#[test]
fn every_body_gives_its_links() {
    let mut failed = Vec::new();
    for case in read::<Links>("links.json") {
        let got = card_links(&case.body);
        if got != case.links {
            failed.push(format!(
                "{} ({:?}): got {got:?}, want {:?}",
                case.name, case.body, case.links
            ));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[derive(Deserialize)]
struct Address {
    ip: String,
    allowed: bool,
    why: String,
}

#[test]
fn every_address_is_judged() {
    let mut failed = Vec::new();
    for case in read::<Address>("addresses.json") {
        let ip: IpAddr = case.ip.parse().unwrap();
        if address_allowed(ip) != case.allowed {
            failed.push(format!(
                "{} ({}): want allowed = {}",
                case.ip, case.why, case.allowed
            ));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
