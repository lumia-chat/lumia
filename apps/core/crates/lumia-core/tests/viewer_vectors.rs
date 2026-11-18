//! Cross-implementation viewer vectors from `proto/vectors/viewer/cases.json` (media spec 5):
//! how a picture is fitted, how far it zooms, and what each move makes of a view. Every
//! client agrees on every vector.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lumia_core::viewer::{self, Frame, View};
use serde::Deserialize;

const PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../proto/vectors/viewer/cases.json"
);

#[derive(Deserialize)]
struct File {
    frames: Vec<Fitted>,
    moves: Vec<Moved>,
    wheel: Vec<Wheel>,
}

#[derive(Deserialize)]
struct Picture {
    own: [f32; 2],
    room: [f32; 2],
    margin: f32,
}

#[derive(Deserialize)]
struct Fitted {
    #[serde(flatten)]
    picture: Picture,
    width: f32,
    height: f32,
    own_scale: f32,
    max_scale: f32,
    detail_scale: f32,
    why: String,
}

#[derive(Deserialize)]
struct At {
    scale: f32,
    x: f32,
    y: f32,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Move {
    ZoomAt { scale: f32, at: [f32; 2] },
    PanBy { by: [f32; 2] },
    Step { steps: i32 },
    Pressed { at: [f32; 2] },
    OwnSize,
    Held,
}

#[derive(Deserialize)]
struct Moved {
    picture: Picture,
    view: At,
    #[serde(rename = "move")]
    by: Move,
    gives: At,
    why: String,
}

#[derive(Deserialize)]
struct Wheel {
    delta: f32,
    pinch: bool,
    factor: f32,
    why: String,
}

fn file() -> File {
    let text = std::fs::read_to_string(PATH).unwrap_or_else(|e| panic!("{PATH}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{PATH}: {e}"))
}

fn frame(p: &Picture) -> Frame {
    viewer::frame(p.own[0], p.own[1], p.room[0], p.room[1], p.margin)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn a_picture_is_fitted_as_the_vectors_say() {
    let file = file();
    assert_eq!(file.frames.len(), 5);
    let failed: Vec<_> = file
        .frames
        .iter()
        .filter(|c| {
            let f = frame(&c.picture);
            !(near(f.width, c.width)
                && near(f.height, c.height)
                && near(f.own, c.own_scale)
                && near(f.max_scale(), c.max_scale)
                && near(f.detail_scale(), c.detail_scale))
        })
        .map(|c| c.why.clone())
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn every_move_gives_what_the_vectors_say() {
    let file = file();
    assert_eq!(file.moves.len(), 24);
    let failed: Vec<_> = file
        .moves
        .iter()
        .filter_map(|c| {
            let f = frame(&c.picture);
            let view = View {
                scale: c.view.scale,
                x: c.view.x,
                y: c.view.y,
            };
            let got = match c.by {
                Move::ZoomAt { scale, at } => viewer::zoom_at(view, scale, at, &f),
                Move::PanBy { by } => viewer::pan_by(view, by[0], by[1], &f),
                Move::Step { steps } => viewer::step(view, steps, &f),
                Move::Pressed { at } => viewer::pressed(view, at, &f),
                Move::OwnSize => viewer::own_size(view, &f),
                Move::Held => viewer::held(view, &f),
            };
            let same =
                near(got.scale, c.gives.scale) && near(got.x, c.gives.x) && near(got.y, c.gives.y);
            (!same).then(|| format!("{}: {got:?}", c.why))
        })
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn the_wheel_zooms_as_the_vectors_say() {
    let file = file();
    assert_eq!(file.wheel.len(), 7);
    let failed: Vec<_> = file
        .wheel
        .iter()
        .filter(|c| !near(viewer::wheel_factor(c.delta, c.pinch), c.factor))
        .map(|c| c.why.clone())
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
