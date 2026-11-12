//! What the spike has to show about text, driven through a windowless `Runtime` over a small
//! seeded store: input-method composition in the composer, and selection across messages.
//! These exercise Rune's handling of the events, not the platform's delivery of them.

use std::sync::Once;

use lumia_rune_spike::data;
use lumia_rune_spike::ui::{self, State};
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

fn runtime() -> Runtime<State> {
    static SEED: Once = Once::new();
    SEED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("lumia-rune-spike-{}", std::process::id()));
        data::seed(&dir, 2, 300).expect("the store seeds");
        // SAFETY: set once, before any test thread reads it (they all pass through this `Once`).
        unsafe { std::env::set_var("LUMIA_SPIKE_DATA", &dir) };
    });
    let mut rt = Runtime::new(ui::init(), ui::view, (1180.0, 760.0), Env::new(true, 2.0));
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn focus_composer(rt: &mut Runtime<State>) {
    let (x, y) = rt.find("composer").expect("the composer is mounted");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
}

#[test]
fn a_composition_is_marked_text_until_it_commits_and_return_confirms_it_rather_than_sending() {
    let mut rt = runtime();
    let before = rt.state.messages.len();
    focus_composer(&mut rt);

    rt.ime(Ime::Preedit("にほんご".into(), Some((12, 12))));
    rt.frame(1.0 / 60.0);
    assert!(rt.state.draft.is_composing());
    assert_eq!(rt.state.draft.text, "", "marked text is drawn, not inserted");

    // Return while composing belongs to the input method (it picks the candidate).
    key(&mut rt, Key::Enter);
    assert_eq!(rt.state.messages.len(), before, "nothing was sent mid-composition");

    rt.ime(Ime::Commit("日本語".into()));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.draft.is_composing());
    assert_eq!(rt.state.draft.text, "日本語");

    key(&mut rt, Key::Enter);
    assert_eq!(rt.state.messages.len(), before + 1, "Return after the commit sends");
    assert_eq!(rt.state.messages[before].body, "日本語");
    assert_eq!(rt.state.draft.text, "");
}

/// Drags from `from` to `to` in eight steps and returns what ends up selected.
fn drag(rt: &mut Runtime<State>, from: (f32, f32), to: (f32, f32)) -> String {
    rt.pointer_move(from.0, from.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(from.0, from.1);
    rt.frame(1.0 / 60.0);
    for step in 1..=8 {
        let t = step as f32 / 8.0;
        rt.pointer_move(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(to.0, to.1);
    rt.frame(1.0 / 60.0);
    rt.selected_text().unwrap_or_default()
}

/// The left edge of message text: rail, sidebar, panel inset, list and row padding, avatar, gap.
const TEXT_LEFT: f32 = 64.0 + 248.0 + 8.0 + 12.0 + 12.0 + 40.0 + 14.0;

#[test]
fn kit_markdown_bodies_select_within_one_message_only() {
    let mut rt = runtime();
    let n = rt.state.messages.len();
    let (first, second) = (rt.state.messages[n - 3].key.clone(), rt.state.messages[n - 2].key.clone());
    let (_, ay) = rt.find(&format!("{first}/body/b0")).expect("the first body is mounted");
    let (_, by) = rt.find(&format!("{second}/body/b0")).expect("the second body is mounted");
    let first_body = rt.state.messages[n - 3].body.clone();

    let within = drag(&mut rt, (TEXT_LEFT + 2.0, ay), (TEXT_LEFT + 60.0, ay));
    assert!(!within.is_empty() && first_body.starts_with(&within), "a drag along a line selects it: {within:?}");

    // Each `Markdown` is a selection group of its own, so the drag stops at the first's end.
    let across = drag(&mut rt, (TEXT_LEFT + 20.0, ay), (TEXT_LEFT + 60.0, by));
    assert!(first_body.ends_with(&across), "the selection stays in the first message: {across:?}");
}

#[test]
fn plain_bodies_in_one_group_select_across_messages() {
    let mut rt = runtime();
    rt.update(|s, _| s.plain = true);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let n = rt.state.messages.len();
    let (first, second) = (rt.state.messages[n - 3].key.clone(), rt.state.messages[n - 2].key.clone());
    let (_, ay) = rt.find(&format!("{first}/body")).expect("the first body is mounted");
    let (_, by) = rt.find(&format!("{second}/body")).expect("the second body is mounted");
    let across = drag(&mut rt, (TEXT_LEFT + 20.0, ay), (TEXT_LEFT + 60.0, by));
    println!("across two messages: {across:?}");
    let first_body = &rt.state.messages[n - 3].body;
    let second_body = &rt.state.messages[n - 2].body;
    let (head, tail) = across.split_once('\n').expect("two messages, joined by a newline");
    assert!(!head.is_empty() && first_body.ends_with(head), "the first message from the press on: {head:?}");
    assert!(!tail.is_empty() && second_body.starts_with(tail), "the second up to the release: {tail:?}");
}

/// Puts the list in one selection group with kit Markdown bodies inside it.
fn joined() -> Runtime<State> {
    let mut rt = runtime();
    rt.update(|s, _| s.joined = true);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

/// Without the kit patch a group around the list changes nothing: a group inside a group is
/// its own, and every kit `Markdown` is one.
#[cfg(not(feature = "kit-patch"))]
#[test]
fn a_group_around_the_list_does_not_join_kit_markdown_bodies() {
    let mut rt = joined();
    let n = rt.state.messages.len();
    let (first, second) = (rt.state.messages[n - 3].key.clone(), rt.state.messages[n - 2].key.clone());
    let (_, ay) = rt.find(&format!("{first}/body/b0")).expect("the first body is mounted");
    let (_, by) = rt.find(&format!("{second}/body/b0")).expect("the second body is mounted");
    let first_body = rt.state.messages[n - 3].body.clone();
    let across = drag(&mut rt, (TEXT_LEFT + 20.0, ay), (TEXT_LEFT + 60.0, by));
    assert!(first_body.ends_with(&across), "the selection stays in the first message: {across:?}");
}

/// With `patches/kit-selection-joined.patch` applied to Rune (the `kit-patch` feature).
#[cfg(feature = "kit-patch")]
mod kit_patch {
    use super::*;

    #[test]
    fn joined_kit_markdown_bodies_select_across_messages() {
        let mut rt = joined();
        let n = rt.state.messages.len();
        let (first, second) = (rt.state.messages[n - 3].key.clone(), rt.state.messages[n - 2].key.clone());
        let (_, ay) = rt.find(&format!("{first}/body/b0")).expect("the first body is mounted");
        let (_, by) = rt.find(&format!("{second}/body/b0")).expect("the second body is mounted");
        let across = drag(&mut rt, (TEXT_LEFT + 20.0, ay), (TEXT_LEFT + 60.0, by));
        println!("first: {:?}\nsecond: {:?}\nacross: {across:?}", rt.state.messages[n - 3].body, rt.state.messages[n - 2].body);
        let (head, tail) = across.split_once('\n').expect("two messages, a break between them");
        assert!(!head.is_empty() && !tail.trim().is_empty());
    }

    /// A drag from the message before a multi-block body (a list, a quote or a code block),
    /// through it, into the message after.
    #[test]
    fn a_selection_runs_through_a_body_of_several_blocks() {
        let mut rt = joined();
        let n = rt.state.messages.len();
        let middle = (1..n - 1)
            .rev()
            .find(|&i| rt.state.messages[i].body.contains("\n") && rt.find(&format!("{}/body/b0", rt.state.messages[i + 1].key)).is_some()
                && rt.find(&format!("{}/body/b0", rt.state.messages[i - 1].key)).is_some())
            .expect("a multi-line body with mounted neighbours");
        let (before, after) = (rt.state.messages[middle - 1].key.clone(), rt.state.messages[middle + 1].key.clone());
        let (_, ay) = rt.find(&format!("{before}/body/b0")).expect("mounted");
        let (_, by) = rt.find(&format!("{after}/body/b0")).expect("mounted");
        let across = drag(&mut rt, (TEXT_LEFT + 2.0, ay), (TEXT_LEFT + 60.0, by));
        println!("source of the middle body:\n{}\n\ncopied:\n{across}", rt.state.messages[middle].body);
        assert!(across.lines().filter(|l| !l.is_empty()).count() >= 3);
    }

    #[test]
    fn select_all_takes_the_whole_channel_including_rows_never_mounted() {
        let mut rt = joined();
        let n = rt.state.messages.len();
        let last = rt.state.messages[n - 1].key.clone();
        let (_, y) = rt.find(&format!("{last}/body/b0")).expect("the newest body is mounted");
        rt.pointer_down(TEXT_LEFT + 4.0, y);
        rt.pointer_up(TEXT_LEFT + 4.0, y);
        rt.frame(1.0 / 60.0);
        rt.key(KeyEvent { key: Key::Command('a'), mods: Mods { cmd: true, ..Mods::default() } });
        rt.frame(1.0 / 60.0);
        let started = std::time::Instant::now();
        let all = rt.selected_text().unwrap_or_default();
        println!("{n} messages, {} bytes copied in {:.1} ms", all.len(), started.elapsed().as_secs_f64() * 1000.0);
        let oldest = rt.state.messages[0].body.lines().next().unwrap_or_default().to_string();
        println!("oldest body's first line: {oldest:?}\nthe copy starts: {:?}", all.chars().take(80).collect::<String>());
        assert!(all.len() > rt.state.messages.iter().map(|m| m.body.len()).sum::<usize>() / 2);
    }
}
