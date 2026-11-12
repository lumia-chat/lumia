//! What the spike has to show about text, driven through GPUI's test window over a small
//! seeded store: input-method composition in the composer, and selection across messages.
//! These exercise GPUI Kit's handling of the text-input protocol and of pointer drags, not
//! the platform's delivery of them. The same checks as `spikes/rune/tests/shell.rs`.

use std::sync::Once;

use gpui_kit::base::TextSelection;
use gpui_kit::component::Root;
use gpui_kit::test::{TestInput, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, ElementId, ElementInputHandler, Entity, Pixels, Point, TestAppContext, VisualTestContext, point, px,
    size,
};
use lumia_gpui_spike::data;
use lumia_gpui_spike::shell::Shell;

fn open(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    static SEED: Once = Once::new();
    SEED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("lumia-gpui-spike-{}", std::process::id()));
        data::seed(&dir, 2, 300).expect("the store seeds");
        // SAFETY: set once, before any test thread reads it (they all pass through this `Once`).
        unsafe { std::env::set_var("LUMIA_SPIKE_DATA", &dir) };
    });
    cx.update(gpui_kit::init);
    let mut shell = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        lumia_gpui_spike::init(true, cx);
        let view = cx.new(|cx| Shell::new(true, window, cx));
        shell = Some(view.clone());
        Root::new(view, window, cx)
    });
    cx.simulate_resize(size(px(1180.), px(760.)));
    cx.update(|window, cx| window.render_frame(cx));
    (shell.expect("the shell was built"), cx)
}

#[gpui_kit::test]
fn a_composition_is_marked_until_it_commits_and_return_then_sends(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let before = shell.read_with(cx, |shell, _| shell.messages.len());
    let composer = shell.read_with(cx, |shell, _| shell.composer.clone());
    cx.update(|window, cx| {
        composer.update(cx, |composer, cx| composer.focus(window, cx));
        window.render_frame(cx);
        let id: ElementId = ("input", composer.entity_id()).into();
        let bounds = window.find(id).bounds();
        let mut input = TestInput::new(ElementInputHandler::new(bounds, composer.clone()), window);

        // GPUI keeps the preedit in the buffer as a marked range, where Rune draws it apart.
        input.compose_text(None, "にほんご", Some(4..4), cx);
        assert_eq!(input.marked_text_range(cx), Some(0..4));
        input.commit_text("日本語", cx);
        assert_eq!(input.marked_text_range(cx), None);
        drop(input);
        assert_eq!(composer.read(cx).value().as_ref(), "日本語");

        window.press("enter", cx);
    });
    shell.read_with(cx, |shell, cx| {
        assert_eq!(shell.messages.len(), before + 1, "Return after the commit sends");
        assert_eq!(shell.messages[before].body.as_ref(), "日本語");
        assert_eq!(shell.composer.read(cx).value().as_ref(), "");
    });
}

/// Where the first line of message `ix`'s body starts: the row's top, its padding and the
/// author line; then rail, sidebar, panel inset and border, list and row padding, avatar, gap.
fn body_start(shell: &Entity<Shell>, ix: usize, cx: &mut VisualTestContext) -> Point<Pixels> {
    let row = shell.read_with(cx, |shell, _| shell.list.bounds_for_item(ix)).expect("the row is laid out");
    point(px(64. + 248. + 8. + 1. + 12. + 12. + 40. + 14.), row.origin.y + px(8. + 24. + 12.))
}

#[gpui_kit::test]
fn a_drag_from_one_message_into_the_next_selects_across_both(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let n = shell.read_with(cx, |shell, _| shell.messages.len());
    let (first, second) =
        shell.read_with(cx, |shell, _| (shell.messages[n - 3].body.to_string(), shell.messages[n - 2].body.to_string()));
    // A list resting at its end reports no row bounds: its scroll top is "past the last
    // item", and `scroll_by` measures from there. One viewport and a point up gives it a
    // first visible row to measure from.
    shell.read_with(cx, |shell, _| shell.list.scroll_by(-(shell.list.viewport_bounds().size.height + px(1.))));
    cx.update(|window, cx| window.render_frame(cx));
    let from = body_start(&shell, n - 3, cx);
    let to = body_start(&shell, n - 2, cx);

    let within = cx.update(|window, cx| {
        window.drag(point(from.x + px(2.), from.y), point(from.x + px(60.), from.y), cx);
        TextSelection::selected_text(window, cx)
    });
    println!("first: {first:?}\nsecond: {second:?}\nwithin one message: {within:?}");
    assert!(!within.trim().is_empty() && first.starts_with(within.trim()), "a drag along a line selects it: {within:?}");

    let across = cx.update(|window, cx| {
        window.drag(point(from.x + px(20.), from.y), point(to.x + px(60.), to.y), cx);
        TextSelection::selected_text(window, cx)
    });
    println!("across two messages: {across:?}");
    let (head, tail) = across.split_once('\n').expect("two messages, joined by a newline");
    assert!(!head.is_empty() && first.ends_with(head.trim_end()), "the first message from the press on: {head:?}");
    assert!(!tail.trim().is_empty() && second.contains(tail.trim().chars().take(3).collect::<String>().as_str()), "the second up to the release: {tail:?}");
}

/// GPUI builds its accessibility tree only once an assistive client connects, and its test
/// window has no way to connect one, so there is no tree to dump headlessly. The kit's test
/// support reads an element's role and name where the element opts in, as its controls do.
#[gpui_kit::test]
fn the_composer_is_exposed_as_a_named_multiline_text_input(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let composer = shell.read_with(cx, |shell, _| shell.composer.clone());
    cx.update(|window, _| {
        let id: ElementId = ("input", composer.entity_id()).into();
        let snapshot = window.find(id);
        println!("composer: {snapshot:?}");
        assert_eq!(snapshot.role(), Some(gpui_kit::Role::MultilineTextInput));
        assert_eq!(snapshot.label(), Some("Message general"));
    });
}
