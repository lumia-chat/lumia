//! Cross-implementation playback vectors from `proto/vectors/playback/cases.json` (media spec
//! 5 and 6.6): what starting something stops, and what stops when its row scrolls away.
//! Every client agrees on every vector.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lumia_core::playback::{stops, stops_off_screen, Kind, Playing};
use serde::Deserialize;

const PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../proto/vectors/playback/cases.json"
);

#[derive(Deserialize)]
struct File {
    starts: Vec<Start>,
    off_screen: Vec<OffScreen>,
}

#[derive(Deserialize)]
struct Thing {
    embed: bool,
    kind: String,
}

#[derive(Deserialize)]
struct Start {
    starting: Thing,
    other: Thing,
    stops: bool,
    why: String,
}

#[derive(Deserialize)]
struct OffScreen {
    playing: Thing,
    stops: bool,
    why: String,
}

fn playing(t: &Thing) -> Playing {
    Playing {
        embed: t.embed,
        kind: Kind::from_id(&t.kind).unwrap_or_else(|| panic!("{} is no kind", t.kind)),
    }
}

fn file() -> File {
    let text = std::fs::read_to_string(PATH).unwrap_or_else(|e| panic!("{PATH}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{PATH}: {e}"))
}

#[test]
fn what_starting_stops_is_as_the_vectors_say() {
    let file = file();
    // Every kind of thing starting, against every kind already playing.
    assert_eq!(file.starts.len(), 25);
    let failed: Vec<_> = file
        .starts
        .iter()
        .filter(|c| stops(playing(&c.starting), playing(&c.other)) != c.stops)
        .map(|c| c.why.clone())
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn what_stops_off_screen_is_as_the_vectors_say() {
    let file = file();
    assert_eq!(file.off_screen.len(), 5);
    let failed: Vec<_> = file
        .off_screen
        .iter()
        .filter(|c| stops_off_screen(playing(&c.playing)) != c.stops)
        .map(|c| c.why.clone())
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
