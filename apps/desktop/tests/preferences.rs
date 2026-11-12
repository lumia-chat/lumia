//! The Preferences page through a windowless `Runtime`: reached from the account menu,
//! three choices that take effect as they are made and are there at the next launch.
//! What the system does with two of them on a Mac (`platform::system`) needs a real
//! launch and is not here.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_shell::app::{HomeView, State};
use lumia_shell::settings::{Appearance, Density, ScrollBars, Settings};
use lumia_shell::tokens;
use rune::{Env, Runtime};

/// The fill of the element keyed `key`: what colour a part of the window is.
fn fill(rt: &Runtime<State>, key: &str) -> rune::Color {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is not on screen"))
        .fill
}

/// Whether a colour has arrived at `wanted`: a change of appearance is a short fade, and
/// the last of it is still under way when the frames here run out.
fn is(color: rune::Color, wanted: rune::Color) -> bool {
    let far = |a: f32, b: f32| (a - b).abs() > 0.005;
    !(far(color.r, wanted.r) || far(color.g, wanted.g) || far(color.b, wanted.b))
}

/// Lets a change of appearance finish.
fn faded(rt: &mut Runtime<State>) {
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
}

/// The rect of the first element keyed `key` under a row keyed `row`.
fn rect(rt: &Runtime<State>, row: &str, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && e.path.contains(&format!("/{row}/")))
        .unwrap_or_else(|| panic!("{row} has no {key}"))
        .rect
}

#[test]
fn the_account_menu_leads_to_preferences_and_each_choice_is_kept() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "self");
    assert!(words(&rt).contains("Preferences"));
    press(&mut rt, "menu-preferences");
    assert_eq!(rt.state.home_view, HomeView::Preferences);
    assert_eq!(rt.state.popover, None, "the menu closed");
    let shown = words(&rt);
    for expected in [
        "Preferences",
        "How Lumia looks on this computer",
        "Appearance",
        "Light, dark, or whichever this computer is in.",
        "Messages",
        "Comfortable gives each message room. Compact shows more of a conversation at once.",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    // As a new computer has them.
    assert_eq!(
        (
            rt.state.settings.appearance,
            rt.state.settings.density,
            rt.state.settings.scroll_bars
        ),
        (
            Appearance::System,
            Density::Comfortable,
            ScrollBars::Automatic
        )
    );

    press(&mut rt, "appearance-0");
    assert_eq!(rt.state.settings.appearance, Appearance::Light);
    press(&mut rt, "appearance-1");
    assert_eq!(rt.state.settings.appearance, Appearance::Dark);
    press(&mut rt, "density-1");
    assert_eq!(rt.state.settings.density, Density::Compact);

    // The scroll bar's choice is offered where the app can carry it out.
    if cfg!(target_os = "macos") {
        assert!(words(&rt).contains(
            "When a list shows its scroll bar. Automatic follows this Mac's own setting."
        ));
        press(&mut rt, "scroll-bars-2");
        assert_eq!(rt.state.settings.scroll_bars, ScrollBars::Always);
        press(&mut rt, "scroll-bars-1");
        assert_eq!(rt.state.settings.scroll_bars, ScrollBars::WhenScrolling);
    } else {
        assert!(rt.find("scroll-bars-card").is_none());
    }

    // Kept: a second launch over the same directory has them, before anything is asked.
    let kept = Settings::load(rt.state.data_dir());
    assert_eq!(
        (kept.appearance, kept.density),
        (Appearance::Dark, Density::Compact)
    );
    let again = State::new(rt.state.data_dir().to_path_buf());
    assert_eq!(again.settings.appearance, Appearance::Dark);
    assert_eq!(again.settings.density, Density::Compact);
    if cfg!(target_os = "macos") {
        assert_eq!(again.settings.scroll_bars, ScrollBars::WhenScrolling);
    }
}

#[test]
fn light_and_dark_are_the_choice_made_and_system_is_whichever_the_computer_is_in() {
    let (light, dark) = (tokens::colors(false).bg, tokens::colors(true).bg);
    assert!(!is(light, dark));
    // A light system: the window is light until told otherwise.
    let mut rt = runtime(signed_in());
    assert!(is(fill(&rt, "shell"), light));
    rt.update(|s, _| s.set_appearance(Appearance::Dark));
    faded(&mut rt);
    assert!(is(fill(&rt, "shell"), dark), "dark on a light system");
    rt.update(|s, _| s.set_appearance(Appearance::System));
    faded(&mut rt);
    assert!(is(fill(&rt, "shell"), light));

    // A dark system: dark, light when chosen, and dark again with the system.
    let mut rt = Runtime::new(
        signed_in(),
        lumia_shell::ui::view,
        (1180.0, 760.0),
        Env::new(true, 2.0),
    );
    faded(&mut rt);
    assert!(is(fill(&rt, "shell"), dark));
    rt.update(|s, _| s.set_appearance(Appearance::Light));
    faded(&mut rt);
    assert!(is(fill(&rt, "shell"), light), "light on a dark system");
    rt.update(|s, _| s.set_appearance(Appearance::System));
    faded(&mut rt);
    assert!(is(fill(&rt, "shell"), dark));
}

#[test]
fn compact_gives_messages_less_room_and_the_same_words() {
    let now = chrono::Utc::now().timestamp_micros();
    let conversation = || {
        let mut state = signed_in();
        state.session.entries = vec![
            message("c0", "grace", "One.", now - 120_000_000),
            message("c1", "grace", "Two.", now - 60_000_000),
            message("c2", "me", "Three.", now),
        ];
        state.session.revision += 1;
        state
    };
    let comfortable = runtime(conversation());
    let mut compact = runtime(conversation());
    compact.update(|s, _| s.set_density(Density::Compact));
    settle(&mut compact);

    let height = |rt: &Runtime<State>| {
        let (first, last) = (rect(rt, "rows", "c0"), rect(rt, "rows", "c2"));
        last[1] + last[3] - first[1]
    };
    assert!(
        height(&compact) < height(&comfortable) - 20.0,
        "the same three messages in less room: {} against {}",
        height(&compact),
        height(&comfortable)
    );
    // A smaller avatar, and the words as they were.
    assert_eq!(rect(&comfortable, "c0", "avatar-c0")[2], 40.0);
    assert_eq!(rect(&compact, "c0", "avatar-c0")[2], 28.0);
    assert_eq!(
        rect(&compact, "c0", "body")[3],
        rect(&comfortable, "c0", "body")[3]
    );
    let said = words(&compact);
    assert!(said.contains("One.") && said.contains("Two.") && said.contains("Three."));
    // Back again.
    compact.update(|s, _| s.set_density(Density::Comfortable));
    settle(&mut compact);
    assert!((height(&compact) - height(&comfortable)).abs() < 1.0);
}
