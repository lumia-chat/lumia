//! The Preferences page through a windowless `Runtime`: reached from the account menu,
//! choices that take effect as they are made and are there at the next launch, and what the
//! message settings do to a conversation's rows.
//! What the system does with two of them on a Mac (`platform::system`) needs a real
//! launch and is not here.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_shell::app::{HomeView, State};
use lumia_shell::settings::{
    Appearance, Avatars, FontSize, Layout, Preset, ScrollBars, Settings, Spacing,
};
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

/// Whether the segment keyed `key` is the one chosen: drawn on the active tab's fill.
fn chosen(rt: &Runtime<State>, key: &str) -> bool {
    is(fill(rt, key), tokens::colors(false).tab_active)
}

/// Which of the preset's three segments is chosen, if any, once a change has faded in.
fn preset_shown(rt: &mut Runtime<State>) -> Option<usize> {
    faded(rt);
    (0..3).find(|i| chosen(rt, &format!("preset-{i}")))
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
        "Start from a preset, then change anything below.",
        "Preset",
        "Comfortable",
        "Compact",
        "Minimal",
        "Layout",
        "Grouped",
        "One line",
        "Spacing",
        "Roomy",
        "Cozy",
        "Tight",
        "Avatars",
        "Large",
        "Small",
        "Hidden",
        "Font size",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    // As a new computer has them: Comfortable at 15.
    let s = &rt.state.settings;
    assert_eq!(
        (s.appearance, s.scroll_bars, s.font_size.points()),
        (Appearance::System, ScrollBars::Automatic, 15)
    );
    assert_eq!(s.display(), Preset::Comfortable.values());
    assert_eq!(preset_shown(&mut rt), Some(0));
    assert!(chosen(&rt, "font-size-2"));
    assert!(rt.find("preset-custom").is_none());
    // Images and embeds have no control while the shell draws neither.
    assert!(!shown.contains("Images") && !shown.contains("Embeds"));

    press(&mut rt, "appearance-0");
    assert_eq!(rt.state.settings.appearance, Appearance::Light);
    press(&mut rt, "appearance-1");
    assert_eq!(rt.state.settings.appearance, Appearance::Dark);

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
    assert_eq!(kept.appearance, Appearance::Dark);
    let again = State::new(rt.state.data_dir().to_path_buf());
    assert_eq!(again.settings.appearance, Appearance::Dark);
    if cfg!(target_os = "macos") {
        assert_eq!(again.settings.scroll_bars, ScrollBars::WhenScrolling);
    }
}

#[test]
fn a_preset_writes_its_values_and_a_change_after_it_is_custom() {
    let mut rt = runtime(signed_in());
    rt.update(|s, _| s.go_home(HomeView::Preferences));
    settle(&mut rt);

    // Each preset writes its values, and is the one shown.
    for (i, preset) in Preset::ALL.into_iter().enumerate() {
        press(&mut rt, &format!("preset-{i}"));
        assert_eq!(rt.state.settings.display(), preset.values(), "{preset:?}");
        assert_eq!(preset_shown(&mut rt), Some(i), "{preset:?}");
        assert!(rt.find("preset-custom").is_none());
    }
    // Minimal is one line, where there are no avatars to choose.
    assert_eq!(rt.state.settings.layout, Layout::OneLine);
    assert!(rt.find("avatars").is_none());
    assert!(!words(&rt).contains("Avatars"));

    // One change after it, and no preset matches: none is shown, and the line says so.
    press(&mut rt, "spacing-0");
    assert_eq!(rt.state.settings.spacing, Spacing::Roomy);
    assert_eq!(preset_shown(&mut rt), None);
    assert!(words(&rt).contains("Custom: these settings match no preset."));

    // Back to grouped: the avatars' choice is there again, and still matches nothing.
    press(&mut rt, "layout-0");
    assert_eq!(rt.state.settings.layout, Layout::Grouped);
    assert!(rt.find("avatars").is_some());
    assert_eq!(preset_shown(&mut rt), None);
    // Large avatars make it Comfortable but for the room: still custom with them...
    press(&mut rt, "avatars-0");
    assert_eq!(rt.state.settings.avatars, Avatars::Large);
    assert_eq!(
        preset_shown(&mut rt),
        None,
        "images and embeds are still links"
    );
    // ...until a preset writes those too.
    press(&mut rt, "preset-0");
    assert_eq!(preset_shown(&mut rt), Some(0));

    // The font size is in no preset: choosing one leaves the preset shown as it was.
    press(&mut rt, "font-size-5");
    assert_eq!(rt.state.settings.font_size.points(), 20);
    assert!(chosen(&rt, "font-size-5"));
    assert_eq!(preset_shown(&mut rt), Some(0));
    press(&mut rt, "preset-1");
    assert_eq!(
        rt.state.settings.font_size.points(),
        20,
        "a preset keeps it"
    );
}

#[test]
fn each_message_setting_is_kept_across_a_launch() {
    let mut rt = runtime(signed_in());
    rt.update(|s, _| s.go_home(HomeView::Preferences));
    settle(&mut rt);
    press(&mut rt, "layout-1");
    press(&mut rt, "spacing-1");
    press(&mut rt, "layout-0");
    press(&mut rt, "avatars-2");
    press(&mut rt, "font-size-4");
    let set = (
        rt.state.settings.display(),
        rt.state.settings.font_size.points(),
    );
    assert_eq!(
        (set.0.layout, set.0.spacing, set.0.avatars, set.1),
        (Layout::Grouped, Spacing::Cozy, Avatars::Hidden, 18)
    );
    let kept = Settings::load(rt.state.data_dir());
    assert_eq!((kept.display(), kept.font_size.points()), set);
    let again = State::new(rt.state.data_dir().to_path_buf());
    assert_eq!(
        (again.settings.display(), again.settings.font_size.points()),
        set
    );
    // And one line, which the presses above left for grouped.
    press(&mut rt, "layout-1");
    let relaunched = State::new(rt.state.data_dir().to_path_buf());
    assert_eq!(relaunched.settings.layout, Layout::OneLine);
    assert_eq!(relaunched.settings.spacing, Spacing::Cozy);
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

/// A channel of three messages, two of them Grace's together, then Ada's, with the message
/// settings `set` makes.
fn conversation(set: impl FnOnce(&mut Settings)) -> Runtime<State> {
    let now = chrono::Utc::now().timestamp_micros();
    let mut state = signed_in();
    state.session.entries = vec![
        message("c0", "grace", "One.", now - 120_000_000),
        message("c1", "grace", "Two.", now - 60_000_000),
        message("c2", "me", "Three.", now),
    ];
    set(&mut state.settings);
    state.session.revision += 1;
    runtime(state)
}

/// The height the three messages take, from the first's top to the last's foot (the day's
/// divider over the first is not counted).
fn height(rt: &Runtime<State>) -> f32 {
    let (first, last) = (
        rect(rt, "rows", "message-c0"),
        rect(rt, "rows", "message-c2"),
    );
    last[1] + last[3] - first[1]
}

#[test]
fn compact_gives_messages_less_room_and_the_same_words() {
    let comfortable = conversation(|_| {});
    let mut compact = conversation(|_| {});
    compact.update(|s, _| s.apply_preset(Preset::Compact));
    settle(&mut compact);

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
    compact.update(|s, _| s.apply_preset(Preset::Comfortable));
    settle(&mut compact);
    assert!((height(&compact) - height(&comfortable)).abs() < 1.0);
    // Tight is less room again than cozy.
    compact.update(|s, _| s.set_spacing(Spacing::Tight));
    settle(&mut compact);
    let tight = height(&compact);
    compact.update(|s, _| s.set_spacing(Spacing::Cozy));
    settle(&mut compact);
    assert!(
        tight < height(&compact),
        "{tight} against {}",
        height(&compact)
    );
}

#[test]
fn one_line_draws_the_time_the_name_and_the_words_on_one_line() {
    let rt = conversation(|s| s.apply(Preset::Minimal));
    let line = tokens::TEXT_MESSAGE.line_height;
    for (row, name) in [
        ("c0", "Grace Hopper"),
        ("c1", "Grace Hopper"),
        ("c2", "Ada Lovelace"),
    ] {
        let (time, author, body) = (
            rect(&rt, row, "time"),
            rect(&rt, row, "author"),
            rect(&rt, row, "body"),
        );
        // In that order along the line, none over the next...
        assert!(time[0] + time[2] <= author[0], "{row}: {time:?} {author:?}");
        assert!(
            author[0] + author[2] <= body[0],
            "{row}: {author:?} {body:?}"
        );
        // ...and level: the time and the name centred on the words' first line.
        let first_line = body[1] + line / 2.0;
        for part in [time, author] {
            let centre = part[1] + part[3] / 2.0;
            assert!(
                (centre - first_line).abs() < 1.0,
                "{row}: {part:?} {body:?}"
            );
        }
        // The name and the time are repeated on every line, even within a run.
        let shown: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| e.path.contains(&format!("/{row}/")))
            .filter_map(|e| e.text)
            .collect();
        assert!(shown.iter().any(|t| t == name), "{row}: {shown:?}");
    }
    // No avatar, and one line each: the three rows take three lines and a little room.
    assert!(rt.find("avatar-c0").is_none());
    assert!(
        height(&rt) < 3.0 * line + 4.0 * tokens::SPACE_4,
        "{}",
        height(&rt)
    );
    // The name opens the person's card, as the avatar does in grouped.
    let mut rt = rt;
    press(&mut rt, "author-c1");
    assert_eq!(rt.state.popover, Some(lumia_shell::app::Popover::Person));
    assert_eq!(
        rt.state.person.as_ref().map(|p| p.account_id.as_str()),
        Some("grace")
    );
}

#[test]
fn a_one_line_message_that_runs_on_wraps_under_its_own_start() {
    let long = "A message long enough to run past the end of its line, so that it has to wrap \
                onto a second and a third, which start under where its first did and not \
                under the time or the name before it. "
        .repeat(3);
    let now = chrono::Utc::now().timestamp_micros();
    let mut state = signed_in();
    state.session.entries = vec![message("w0", "grace", &long, now)];
    state.settings.apply(Preset::Minimal);
    state.session.revision += 1;
    let rt = runtime(state);
    let body = rect(&rt, "w0", "body");
    let author = rect(&rt, "w0", "author");
    assert!(
        body[3] > 2.0 * tokens::TEXT_MESSAGE.line_height,
        "it wrapped: {body:?}"
    );
    assert!(body[0] >= author[0] + author[2], "{body:?} {author:?}");
}

#[test]
fn hidden_avatars_leave_no_gutter_and_the_name_opens_the_card() {
    let mut rt = conversation(|s| s.avatars = Avatars::Hidden);
    assert!(rt.find("avatar-c0").is_none());
    let frame = rect(&rt, "rows", "message-c0");
    let (author, body) = (rect(&rt, "c0", "author"), rect(&rt, "c0", "body"));
    // The name starts where the avatar did, at the row's padding, and the words under it.
    assert_eq!(author[0] - frame[0], tokens::SPACE_12);
    assert_eq!(body[0], author[0]);
    // A later message of the block is under them, with no time in a column before it; its
    // time is in its label.
    let later = rect(&rt, "c1", "body");
    assert_eq!(later[0], body[0]);
    let label = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("message-c1"))
        .and_then(|e| e.label)
        .unwrap_or_default();
    assert!(label.starts_with("Grace Hopper, "), "{label:?}");
    // The name opens the card.
    press(&mut rt, "author-c0");
    assert_eq!(rt.state.popover, Some(lumia_shell::app::Popover::Person));
    assert_eq!(
        rt.state.person.as_ref().map(|p| p.account_id.as_str()),
        Some("grace")
    );
}

#[test]
fn the_font_size_changes_the_text_and_not_the_avatars() {
    let at_15 = conversation(|_| {});
    let at_20 = conversation(|s| s.font_size = FontSize::new(20).unwrap());
    let (small, large) = (rect(&at_15, "c0", "body"), rect(&at_20, "c0", "body"));
    let ratio = large[3] / small[3];
    assert!(
        (ratio - 20.0 / 15.0).abs() < 0.05,
        "the body's line at 20 against 15: {} against {}",
        large[3],
        small[3]
    );
    for part in ["author", "time"] {
        assert!(
            rect(&at_20, "c0", part)[3] > rect(&at_15, "c0", part)[3],
            "{part} grew"
        );
    }
    assert_eq!(rect(&at_20, "c0", "avatar-c0")[2], 40.0);
    // The composer's words grow with them.
    let composer = |rt: &Runtime<State>| {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("composer"))
            .map(|e| e.rect[3])
            .unwrap()
    };
    assert!(composer(&at_20) > composer(&at_15));
    // The same in one line.
    let line_15 = conversation(|s| s.apply(Preset::Minimal));
    let line_20 = conversation(|s| {
        s.apply(Preset::Minimal);
        s.font_size = FontSize::new(20).unwrap();
    });
    assert!(height(&line_20) > height(&line_15) + 3.0 * 5.0);
}
