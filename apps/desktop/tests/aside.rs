//! The aside beside a conversation (`app::aside`): a server's member list, put away and
//! brought back by the header's toggle in a wide window, and slid over the conversation in
//! a narrow one, where a press outside it or Escape puts it away.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_shell::app::State;
use lumia_shell::settings::Settings;
use lumia_shell::ui;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn shown(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn toggle(rt: &Runtime<State>) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("toggle-aside"))
        .expect("the header has the toggle")
}

#[test]
fn a_wide_window_puts_the_member_list_away_and_remembers_it() {
    let mut rt = runtime(signed_in());
    assert!(shown(&rt, "members"));
    assert_eq!(toggle(&rt).label.as_deref(), Some("Hide members"));

    press(&mut rt, "toggle-aside");
    assert!(!shown(&rt, "members"), "put away");
    assert_eq!(toggle(&rt).label.as_deref(), Some("Show members"));
    assert!(
        Settings::load(rt.state.data_dir()).aside_hidden,
        "and remembered"
    );

    press(&mut rt, "toggle-aside");
    assert!(shown(&rt, "members"));
    assert!(!Settings::load(rt.state.data_dir()).aside_hidden);
}

#[test]
fn a_narrow_window_slides_the_member_list_over_and_puts_it_away() {
    let mut rt = Runtime::new(signed_in(), ui::view, (900.0, 700.0), Env::new(false, 2.0));
    settle(&mut rt);
    assert!(rt.state.narrow());
    assert!(!shown(&rt, "members"), "no room for it in the row");
    assert_eq!(toggle(&rt).label.as_deref(), Some("Show members"));

    press(&mut rt, "toggle-aside");
    assert!(shown(&rt, "aside-over") && shown(&rt, "members"));
    // Over the conversation's end: its right edge at the window's, less the frame's 8.
    let over = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("aside-over"))
        .unwrap();
    // Where layout puts it (`target`): the slide may still be on its way there.
    assert!(
        (over.target[0] + over.target[2] - 900.0).abs() < 1.0,
        "{:?}",
        over.target
    );

    // A press on the shade puts it away, and the remembered choice is not touched.
    press(&mut rt, "aside-scrim");
    assert!(!shown(&rt, "aside-over"));
    assert!(!Settings::load(rt.state.data_dir()).aside_hidden);

    // And Escape does.
    press(&mut rt, "toggle-aside");
    assert!(shown(&rt, "aside-over"));
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    settle(&mut rt);
    assert!(!shown(&rt, "aside-over"));

    // Widened past the breakpoint, the list is back in the row and nothing is over.
    press(&mut rt, "toggle-aside");
    rt.set_viewport((1300.0, 760.0), 2.0);
    settle(&mut rt);
    assert!(!rt.state.narrow());
    assert!(shown(&rt, "members") && !shown(&rt, "aside-over"));
}
