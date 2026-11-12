//! The messages of the conversation on screen: a virtualized list of the session's entries
//! and the sends the store does not hold yet, with a divider above each day's first message.
//! The same list draws a server's channel and a personal conversation.
//!
//! The list is one selection group: a drag runs from one message into the next, Cmd+A in
//! a message takes the conversation, and a copy reads as a transcript. Each stored message
//! gives it its author and time on one line (a tab between them, as Rune joins texts side
//! by side), its body under that (`body` says what a body copies as), and a blank line
//! before the next message. Without the names a copied stretch of conversation does not
//! say who said what; a selection inside one body takes neither. The day dividers stay out
//! of it: "Today" is wrong by tomorrow. A send still pending is not selectable at all.
//!
//! Two limits are Rune's (its issue 550): a copy that spans rows scrolled out of the
//! mounted window builds them to read their text and stops, without saying so, at 10,000
//! such rows or 4 MiB; and while a selection spans many unmounted rows, each scrolled
//! frame that mounts rows costs more (`benches/timeline.rs` measures it).

use std::borrow::Cow;

use chrono::{DateTime, Local, NaiveDate};
use rune::{Cursor, El, GrowthSpec, ScrollSpec, col, el, row, text};

use super::body;
use super::components::{Look, avatar};
use crate::app::State;
use crate::session::{Entry, Pending};
use crate::tokens::{self as t};

/// How close to the oldest row on screen the reader gets before the page before it is read.
const OLDER_WITHIN: usize = 20;

/// A message row's height before it is measured: an avatar, its padding and the gap under it.
const ROW_ESTIMATE: f32 = t::AVATAR_LG + t::SPACE_24 + t::SPACE_2;

/// The list, pinned to its newest message. `label` names it for assistive technology.
/// Call it only when there is something to list.
pub fn view(s: &State, label: String, look: Look) -> El {
    let count = s.session.entries.len() + s.session.pending_here().count();
    let dark = look.dark;
    let rows = col()
        .key("rows")
        .w_full()
        .pt(t::SPACE_14)
        .pb(t::SPACE_8)
        .px(t::SPACE_14)
        .growth(GrowthSpec::default())
        .role("list")
        .label(label)
        .selection_group(true)
        .virtual_rows(
            count,
            ROW_ESTIMATE,
            |s: &State, i| match s.session.entries.get(i) {
                Some(entry) => Cow::Borrowed(entry.id.as_str()),
                None => s
                    .session
                    .pending_here()
                    .nth(i - s.session.entries.len())
                    .map_or(Cow::Borrowed(""), |p| Cow::Borrowed(p.id.as_str())),
            },
            move |s: &State, i| {
                let look = Look::of(dark);
                match s.session.entries.get(i) {
                    Some(entry) => stored_row(s, i, entry, look),
                    None => match s.session.pending_here().nth(i - s.session.entries.len()) {
                        Some(pending) => pending_row(s, pending, look),
                        None => el(),
                    },
                }
            },
        )
        .virtual_rows_version(s.session.revision)
        // Reading up to the oldest rows on screen asks for the page before them.
        .on_visible_rows(|s: &mut State, visible| visible.start < OLDER_WITHIN && s.load_older());
    // A conversation shorter than the pane sits at its foot, against the composer, as the
    // board's does: the space above the first message takes what the rows leave.
    col()
        .key("messages")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        .child(el().grow(1.0))
        .child(rows)
}

/// Whether the open conversation has anything to list.
pub fn is_empty(s: &State) -> bool {
    s.session.entries.is_empty() && s.session.pending_here().next().is_none()
}

fn local(micros: i64) -> DateTime<Local> {
    DateTime::from_timestamp_micros(micros)
        .unwrap_or_default()
        .with_timezone(&Local)
}

fn clock(micros: i64) -> String {
    local(micros).format("%-I:%M %p").to_string()
}

/// How a day reads above its first message.
fn day_label(day: NaiveDate) -> String {
    let today = Local::now().date_naive();
    if day == today {
        "Today".into()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".into()
    } else {
        day.format("%b %-d").to_string()
    }
}

fn day_divider(label: String, look: Look) -> El {
    let c = look.c;
    let rule = || el().grow(1.0).h(1.0).fill(c.line);
    row()
        .w_full()
        .px(t::SPACE_10)
        .py(t::SPACE_8)
        .gap(t::SPACE_12)
        .items_center()
        .child(rule())
        .child(text(label).font(t::TEXT_SECTION).color(c.ink_quiet))
        .child(rule())
}

/// An avatar beside a name, a time, and whatever the message is. `selectable` puts the name
/// and the time in the list's selection: a copy that reaches them starts the message on a
/// line of its own, after a blank one.
fn message_frame(
    key: &str,
    author: &str,
    author_id: &str,
    time: String,
    content: El,
    selectable: bool,
    look: Look,
) -> El {
    let c = look.c;
    let part = |part: El, before: u8| {
        if selectable {
            part.selectable(true)
                .selection_color(c.accent_soft)
                .copy_breaks(before, 1)
        } else {
            part
        }
    };
    // The avatar opens its person's card, as the board's does. The name stays text: it is
    // part of what a selection copies, and a press on it starts one.
    let mut picture = el().child(avatar(author, author_id, t::AVATAR_LG, look));
    if !author_id.is_empty() {
        let anchor = format!("avatar-{key}");
        let (id, at) = (author_id.to_string(), anchor.clone());
        picture = picture
            .key(anchor)
            .radius(t::AVATAR_LG / 2.0)
            .role("button")
            .label(format!("{author}, profile"))
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(move |s: &mut State| s.open_person(&id, &at));
    }
    let name = text(author).key("author").font(t::TEXT_AUTHOR).color(c.ink);
    let time = text(time).key("time").font(t::TEXT_META).color(c.ink_muted);
    let head = row()
        .gap(t::SPACE_8)
        .items_center()
        .child(part(name, 2))
        .child(part(time, 1));
    row()
        .key(key)
        .w_full()
        .px(t::SPACE_12)
        .py(t::SPACE_8)
        // The board's gap between one row and the next.
        .mb(t::SPACE_2)
        .gap(t::SPACE_14)
        .radius(t::RADIUS_12)
        .hover(c.hover_message)
        .role("listitem")
        .child(picture)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(t::SPACE_4)
                .child(head)
                .child(content),
        )
}

fn stored_row(s: &State, i: usize, entry: &Entry, look: Look) -> El {
    let c = look.c;
    let author = s.session.name_of(&entry.author_account_id);
    let content = if entry.deleted {
        body::note("(deleted)", c.ink_quiet, look)
    } else {
        body::view(&entry.body, true, look)
    };
    let row = message_frame(
        &entry.id,
        &author,
        &entry.author_account_id,
        clock(entry.at_micros),
        content.key("body"),
        true,
        look,
    );
    // The divider lives in the first row of its day: a row of its own would vanish when the
    // list unmounts it.
    let day = local(entry.at_micros).date_naive();
    let previous = i
        .checked_sub(1)
        .and_then(|p| s.session.entries.get(p))
        .map(|e| local(e.at_micros).date_naive());
    // So does the word that older messages are on their way, in the oldest row.
    let fetching = i == 0 && s.session.loading_older();
    if previous == Some(day) && !fetching {
        return row;
    }
    let mut above = col().key(format!("day-{}", entry.id)).w_full();
    if fetching {
        above = above.child(
            text("Loading earlier messages…")
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .w_full()
                .align(rune::Align::Center)
                .py(t::SPACE_8)
                .role("status"),
        );
    }
    if previous != Some(day) {
        above = above.child(day_divider(day_label(day), look));
    }
    above.child(row)
}

/// A send the store does not hold yet: dimmed until it does, with the refusal when there was one.
fn pending_row(s: &State, pending: &Pending, look: Look) -> El {
    let c = look.c;
    let body = body::view(&pending.body, false, look).key("body");
    let content = match &pending.failed {
        None => col().w_full().opacity(0.5).child(body),
        Some(why) => {
            let id = pending.id.clone();
            let dismiss = el()
                .key("dismiss")
                .role("button")
                .label("Dismiss")
                .cursor(Cursor::Pointer)
                .focusable(true)
                .focus_ring(2.0, 2.0, c.focus)
                .on_press(move |s: &mut State| s.session.dismiss(&id))
                .child(text("Dismiss").font(t::TEXT_LINK_SM).color(c.link));
            col()
                .w_full()
                .gap(t::SPACE_4)
                .child(el().w_full().opacity(0.5).child(body))
                .child(
                    row()
                        .gap(t::SPACE_8)
                        .items_center()
                        .child(
                            text(format!("Not sent: {why}"))
                                .font(t::TEXT_META)
                                .color(c.dnd),
                        )
                        .child(dismiss),
                )
        }
    };
    let time = if pending.failed.is_some() {
        clock(pending.sent_at_micros)
    } else {
        "Sending…".into()
    };
    message_frame(
        &pending.id,
        s.session.self_name(),
        s.session.me(),
        time,
        content,
        false,
        look,
    )
}
