//! The conversation panel: a header, the timeline and the composer. A server's channel and a
//! personal conversation are both drawn by it; [`Chrome`] is what differs between them.

use rune::{Cursor, Cycle, El, Icon, Span, col, el, icon, rich, row, text, text_input};

use super::components::{Look, empty_state, error_text, panel, title_rule, topic as topic_line};
use super::timeline;
use crate::app::State;
use crate::tokens::{self as t, icons};

/// What a conversation's panel says around its messages.
pub struct Chrome<'a> {
    pub title: &'a str,
    /// The glyph before the title: the hash of a channel, or none.
    pub glyph: Option<&'static Icon>,
    /// The list's name for assistive technology (`Messages in general`).
    pub list_label: String,
    /// The composer's placeholder and name (`Message #general`).
    pub placeholder: String,
    /// The title and line of the empty state, when there is nothing to list.
    pub empty: (&'a str, &'a str),
}

/// What a personal conversation says besides; a channel has none of it.
#[derive(Default)]
pub struct Extras {
    /// The line beside the title (the other side's pronouns, the host).
    pub topic: Option<String>,
    /// What to know before reading, between the header and the messages.
    pub notices: Vec<El>,
    /// The glyph of the empty state, when it is not a channel's hash.
    pub empty_glyph: Option<&'static Icon>,
    /// The composer takes no words yet; the placeholder says why.
    pub composer_disabled: bool,
}

/// The panel for the conversation on screen.
pub fn view(s: &State, chrome: Chrome<'_>, look: Look) -> El {
    view_with(s, chrome, Extras::default(), look)
}

/// [`view`] with what a personal conversation adds.
pub fn view_with(s: &State, chrome: Chrome<'_>, extras: Extras, look: Look) -> El {
    let list = if timeline::is_empty(s) {
        empty_state(
            extras.empty_glyph.unwrap_or(&icons::HASH),
            chrome.empty.0,
            chrome.empty.1,
            look,
        )
    } else {
        timeline::view(s, chrome.list_label, look)
    };
    let mut pane = panel("panel", look).w_full().child(header_with(
        chrome.title,
        chrome.glyph,
        extras.topic.as_deref(),
        look,
    ));
    if !extras.notices.is_empty() {
        let mut notices = col()
            .key("notices")
            .w_full()
            .px(t::SPACE_24)
            .pt(t::SPACE_12)
            .gap(t::SPACE_8);
        for notice in extras.notices {
            notices = notices.child(notice);
        }
        pane = pane.child(notices);
    }
    pane = pane.child(list);
    if let Some(error) = &s.session.last_error {
        pane = pane.child(el().w_full().px(t::SPACE_24).child(error_text(error, look)));
    }
    pane.child(if extras.composer_disabled {
        composer_off(&chrome.placeholder, look)
    } else {
        composer(s, &chrome.placeholder, look)
    })
}

/// A panel with a header and an empty state, for a place with no conversation.
pub fn vacant(title: &str, glyph: &'static Icon, empty: (&str, &str), look: Look) -> El {
    panel("panel", look)
        .w_full()
        .child(header(title, None, look))
        .child(empty_state(glyph, empty.0, empty.1, look))
}

/// The panel's header: an optional glyph and the title, over a hairline.
pub fn header(title: &str, glyph: Option<&'static Icon>, look: Look) -> El {
    header_with(title, glyph, None, look)
}

/// [`header`] with a topic: a quieter line beside the title, after a rule.
pub fn header_with(
    title: &str,
    glyph: Option<&'static Icon>,
    topic: Option<&str>,
    look: Look,
) -> El {
    let c = look.c;
    let mut bar = row()
        .key("header")
        .w_full()
        .h(t::HEADER_HEIGHT)
        .pl(t::SPACE_24)
        .pr(t::SPACE_14)
        .gap(t::SPACE_8)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .window_drag();
    if let Some(glyph) = glyph {
        bar = bar.child(icon(glyph, t::HEADER_GLYPH).color(c.accent));
    }
    bar = bar.child(
        text(title)
            .font(t::TITLE_CHANNEL)
            .color(c.ink)
            .truncate()
            .role("heading"),
    );
    if let Some(topic) = topic.filter(|topic| !topic.is_empty()) {
        bar = bar.child(title_rule(look)).child(topic_line(topic, look));
    }
    bar
}

/// The composer of a conversation that takes no words yet: its frame, with the reason where
/// the words would go.
fn composer_off(reason: &str, look: Look) -> El {
    let c = look.c;
    let frame = row()
        .key("composer-box")
        .w_full()
        .items_center()
        .gap(t::SPACE_6)
        .pl(t::SPACE_16)
        .pr(t::SPACE_6)
        .py(t::SPACE_6)
        .radius(t::RADIUS_16)
        .fill(c.field)
        .border(1.0, c.border_composer)
        .border_layout()
        .child(
            text(reason)
                .key("composer")
                .font(t::TEXT_MESSAGE)
                .color(c.ink_muted)
                .truncate()
                .grow(1.0)
                .min_w(0.0)
                .role("textbox")
                .label(reason)
                .disabled(true),
        )
        .child(
            el().h(t::COMPOSER_LINE).center().child(
                el().key("send")
                    .size(t::SEND_SIZE, t::SEND_SIZE)
                    .radius(t::SEND_SIZE / 2.0)
                    .fill(c.accent)
                    .center()
                    .opacity(0.5)
                    .role("button")
                    .label("Send")
                    .disabled(true)
                    .child(icon(&icons::SEND, t::SPACE_18).color(c.on_accent)),
            ),
        );
    col()
        .w_full()
        .px(t::SPACE_24)
        .pt(t::SPACE_4)
        .pb(t::SPACE_14)
        .child(frame)
}

/// The typing hint's dots, as the board's `qsDot` keyframes: each brightens and dims in
/// turn, the second and third 0.15 s behind the one before. Still, and whole, under
/// Reduce Motion.
const DOT_STOPS: &[(f32, f32)] = &[(0.0, 0.3), (0.4, 1.0), (0.8, 0.3), (1.0, 0.3)];
const DOT_PERIOD: f32 = 1.2;
const DOT_LAG: f32 = 0.15;

/// What the hint says of those typing, and how much of it from its start is their names.
fn typing_words(names: &[String]) -> (String, usize) {
    match names {
        [] => (String::new(), 0),
        [one] => (format!("{one} is typing…"), one.len()),
        [one, other] => {
            let both = format!("{one} and {other}");
            let named = both.len();
            (format!("{both} are typing…"), named)
        }
        _ => ("Several people are typing…".into(), 0),
    }
}

/// Who is typing: three dots and the words, the names in stronger ink. Protocol 6.7; a
/// personal conversation never has one (the session says why).
fn typing_hint(names: &[String], look: Look) -> El {
    let c = look.c;
    let mut dots = row().gap(t::TYPING_DOT_GAP).a11y_hidden();
    for n in 0..3u8 {
        dots = dots.child(
            el().key(format!("dot{n}"))
                .size(t::TYPING_DOT, t::TYPING_DOT)
                .radius(t::TYPING_DOT / 2.0)
                .fill(c.ink_muted)
                .cycle(Cycle {
                    period: DOT_PERIOD,
                    phase: f32::from(n) * DOT_LAG / DOT_PERIOD,
                    stops: DOT_STOPS,
                    still: 1.0,
                    fps: 30.0,
                }),
        );
    }
    let (words, named) = typing_words(names);
    let names = Span::new(0, named)
        .weight(t::TEXT_SECTION.weight)
        .color(c.ink_emphasis);
    row()
        .key("typing")
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_8)
        .items_center()
        .child(dots)
        .child(
            rich(words, vec![names])
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        )
}

/// The line under the composer: who is typing at its start, as the board has it, and
/// what Return does at its end.
fn composer_hint(s: &State, look: Look) -> El {
    let typing = s.session.typing_here();
    let mut line = row()
        .w_full()
        .px(t::SPACE_8)
        .pt(t::SPACE_8)
        .gap(t::SPACE_12)
        .items_center();
    line = if typing.is_empty() {
        line.child(el().grow(1.0))
    } else {
        line.child(typing_hint(&typing, look))
    };
    line.child(
        text("Return to send")
            .font(t::TEXT_META)
            .color(look.c.ink_muted),
    )
}

fn composer(s: &State, placeholder: &str, look: Look) -> El {
    let c = look.c;
    let accent = c.accent;
    let can_send = !s.draft.text.trim().is_empty();
    let input = text_input(&s.draft, t::TEXT_MESSAGE)
        .key("composer")
        .label(placeholder)
        .placeholder(placeholder, c.ink_muted)
        .color(c.ink_field)
        .caret(accent, c.accent_soft)
        .w_full()
        .min_h(t::SPACE_24)
        .max_h(t::SPACE_24 * 8.0)
        .on_edit(|s: &mut State| &mut s.draft)
        .on_submit(|s: &mut State, tx| s.send(tx));
    let mut send = el()
        .key("send")
        .size(t::SEND_SIZE, t::SEND_SIZE)
        .radius(t::SEND_SIZE / 2.0)
        .fill(accent)
        .center()
        .role("button")
        .label("Send")
        .disabled(!can_send)
        .child(icon(&icons::SEND, t::SPACE_18).color(c.on_accent));
    // The board draws the button at full strength over an empty field, so it is not
    // dimmed here either; it is only inert until there is something to send.
    if can_send {
        send = send
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press_tx(|s: &mut State, tx| s.send(tx));
    }
    let mut frame = row()
        .key("composer-box")
        .w_full()
        .items_end()
        .gap(t::SPACE_6)
        .pl(t::SPACE_16)
        .pr(t::SPACE_6)
        .py(t::SPACE_6)
        .radius(t::RADIUS_16)
        .fill(c.field)
        .border(1.0, c.border_composer)
        // The border takes room, as the board's does: 56 points with one line in it.
        .border_layout()
        .focus_within_ring(1.0, -1.0, c.border_active)
        // One line is the board's 42 points with the words and the button centred in it; more
        // lines grow the frame upwards from the button.
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .min_h(t::COMPOSER_LINE)
                .justify_center()
                .child(input),
        )
        .child(el().h(t::COMPOSER_LINE).center().child(send));
    for layer in look.shadows.composer {
        frame = frame.box_shadow(*layer);
    }
    col()
        .w_full()
        .px(t::SPACE_24)
        .pt(t::SPACE_4)
        .pb(t::SPACE_14)
        .child(frame)
        .child(composer_hint(s, look))
}
