//! The messages of the conversation on screen: a virtualized list of the session's entries
//! and the sends the store does not hold yet, with a divider above each day's first message.
//! The same list draws a server's channel and a personal conversation.
//!
//! Messages one person sends close together are one block, as the board's are: the avatar,
//! the name and the time once, over the first, and the rest under it at a paragraph's
//! distance ([`joins`] says what "close together" is). Each message is still a row of its
//! own, so the list unmounts a long block a row at a time; a row after the first has its
//! time in the avatar's column, shown while the pointer is on the row.
//!
//! The list is one selection group: a drag runs from one message into the next, Cmd+A in
//! a message takes the conversation, and a copy reads as a transcript. Each block gives it
//! its author and time on one line (a tab between them, as Rune joins texts side by side),
//! each message's body under that (`body` says what a body copies as) with a blank line
//! between two, and a blank line before the next block. Without the names a copied stretch
//! of conversation does not say who said what; a selection inside one body takes neither.
//! The day dividers stay out of it: "Today" is wrong by tomorrow. A send still pending is
//! not selectable at all.
//!
//! Two limits are Rune's (its issue 550): a copy that spans rows scrolled out of the
//! mounted window builds them to read their text and stops, without saying so, at 10,000
//! such rows or 4 MiB; and while a selection spans many unmounted rows, each scrolled
//! frame that mounts rows costs more (`benches/timeline.rs` measures it).

use std::borrow::Cow;

use chrono::{DateTime, Local, NaiveDate};
use rune::{Cursor, El, GrowthSpec, ScrollSpec, col, el, icon, row, text};

use super::body;
use super::components::{Look, avatar_of};
use super::scroll::WithScrollBar;
use crate::app::State;
use crate::session::{Entry, Pending};
use crate::settings::Density;
use crate::tokens::{self as t, icons};

/// How close to the oldest row on screen the reader gets before the page before it is read.
const OLDER_WITHIN: usize = 20;

/// A row's measures, which the density chosen in Preferences decides. Comfortable is the
/// boards'; compact is the same row with a smaller avatar and less room around it, so more
/// of a conversation fits. The text is the same size in both.
#[derive(Clone, Copy)]
struct Measures {
    avatar: f32,
    /// The room over a block's first message and under its last, inside the row.
    edge: f32,
    /// The room between one block and the next. The board first drew 2 here, which read
    /// as crowded in the app; Ryan asked for a little more (2026-11-12) and the boards
    /// follow.
    block_gap: f32,
    /// Between the avatar and what is beside it.
    beside: f32,
    /// Between the name and the message under it.
    under_name: f32,
    /// Over and under a day's divider.
    divider: f32,
}

impl Measures {
    fn of(density: Density) -> Self {
        match density {
            Density::Comfortable => Self {
                avatar: t::AVATAR_LG,
                edge: t::SPACE_8,
                block_gap: t::SPACE_8,
                beside: t::SPACE_14,
                under_name: t::SPACE_4,
                divider: t::SPACE_8,
            },
            Density::Compact => Self {
                avatar: t::AVATAR_SM,
                edge: t::SPACE_4,
                block_gap: t::SPACE_4,
                beside: t::SPACE_10,
                under_name: t::SPACE_2,
                divider: t::SPACE_4,
            },
        }
    }

    /// A message row's height before it is measured: a name over one line of message,
    /// or the avatar where that is taller, with the row's edges and the gap under it.
    fn row_estimate(self) -> f32 {
        let words = t::TEXT_MESSAGE.line_height * 2.0 + self.under_name;
        words.max(self.avatar) + 2.0 * self.edge + self.block_gap
    }
}

/// How long after a message another from the same person still goes in its block.
const BLOCK_WITHIN_MICROS: i64 = 5 * 60 * 1_000_000;

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
            Measures::of(s.settings.density).row_estimate(),
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
                let place = Place::of(s, i);
                match s.session.entries.get(i) {
                    Some(entry) => stored_row(s, i, entry, place, look),
                    None => match s.session.pending_here().nth(i - s.session.entries.len()) {
                        Some(pending) => pending_row(s, pending, place, look),
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
        .with_scroll_bar("messages", look)
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

/// Who wrote row `i` and when, whether the store holds it or it is still being sent. A
/// notice has no one, so it joins no block and opens none.
fn stamp(s: &State, i: usize) -> Option<(&str, i64)> {
    match s.session.entries.get(i) {
        Some(entry) if !entry.notice.is_empty() => Some(("", entry.at_micros)),
        Some(entry) => Some((entry.author_account_id.as_str(), entry.at_micros)),
        None => s
            .session
            .pending_here()
            .nth(i - s.session.entries.len())
            .map(|pending| (s.session.me(), pending.sent_at_micros)),
    }
}

/// Whether a message goes in the block of the one before it: the same person, on the same
/// day (a day's first message carries the divider), within [`BLOCK_WITHIN_MICROS`] of it.
/// Measured from the message before, not the block's first, so a run of quick messages
/// stays one block however long it goes on. Either way round, since a pending send is
/// stamped by this computer's clock and the message before it by the instance's.
fn joins((before, before_at): (&str, i64), (author, at): (&str, i64)) -> bool {
    !author.is_empty()
        && author == before
        && (at - before_at).abs() < BLOCK_WITHIN_MICROS
        && local(at).date_naive() == local(before_at).date_naive()
}

/// Where a row sits in its block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    /// The block's first message: the one under the avatar, the name and the time.
    opens: bool,
    /// Its last: the next row, when there is one, opens another.
    closes: bool,
}

impl Place {
    fn of(s: &State, i: usize) -> Self {
        let at = |i: usize| stamp(s, i);
        let joined = |i: usize| {
            i.checked_sub(1)
                .and_then(at)
                .zip(at(i))
                .is_some_and(|(before, this)| joins(before, this))
        };
        Self {
            opens: !joined(i),
            closes: !joined(i + 1),
        }
    }
}

/// A time without its half of the day, for the avatar's column: `9:04`.
fn short_clock(micros: i64) -> String {
    local(micros).format("%-I:%M").to_string()
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

fn day_divider(label: String, m: Measures, look: Look) -> El {
    let c = look.c;
    let rule = || el().grow(1.0).h(1.0).fill(c.line);
    row()
        .w_full()
        .px(t::SPACE_10)
        .py(m.divider)
        .gap(t::SPACE_12)
        .items_center()
        .child(rule())
        .child(text(label).font(t::TEXT_SECTION).color(c.ink_quiet))
        .child(rule())
}

/// What a message row says besides its content.
struct Head<'a> {
    author: &'a str,
    author_id: &'a str,
    /// The time beside the name, or what stands there for a send on its way.
    time: String,
    /// The time in the avatar's column of a row that goes on in a block.
    short_time: String,
    /// Puts the name and the time in the list's selection: a copy that reaches them starts
    /// the block on a line of its own, after a blank one.
    selectable: bool,
}

/// A message's row. The first of a block is an avatar beside a name, a time and the
/// message; the ones after it are the message alone under those, in the same column.
fn message_frame(
    s: &State,
    key: &str,
    head: Head<'_>,
    content: El,
    place: Place,
    look: Look,
) -> El {
    let c = look.c;
    let Head {
        author,
        author_id,
        time,
        short_time,
        selectable,
    } = head;
    let m = Measures::of(s.settings.density);
    // A block's rows touch, and are a paragraph's distance apart between them: the board
    // draws a block's messages as the paragraphs of one.
    let within = m.under_name / 2.0;
    let frame = row()
        .key(key)
        .w_full()
        .px(t::SPACE_12)
        .pt(if place.opens { m.edge } else { within })
        .pb(if place.closes { m.edge } else { within })
        .gap(m.beside)
        .radius(t::RADIUS_12)
        .hover(c.hover_message)
        .role("listitem");
    if !place.opens {
        // The time of a message under the block's first, where the avatar would be: there
        // for assistive technology always, and for the eye while the pointer is on the row.
        let when = el()
            .w(m.avatar)
            .h(t::TEXT_MESSAGE.line_height)
            .center()
            .child(
                text(short_time)
                    .key("time")
                    .font(t::TEXT_META)
                    .color(c.ink_muted)
                    .label(format!("{author}, {time}"))
                    .group_opacity(0.0, 1.0),
            );
        return frame
            .hover_group()
            .child(when)
            .child(col().grow(1.0).min_w(0.0).child(content));
    }
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
    let mut picture = el().child(avatar_of(s, author, author_id, m.avatar, look));
    if !author_id.is_empty() {
        let anchor = format!("avatar-{key}");
        let (id, at) = (author_id.to_string(), anchor.clone());
        picture = picture
            .key(anchor)
            .radius(m.avatar / 2.0)
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
    frame.child(picture).child(
        col()
            .grow(1.0)
            .min_w(0.0)
            .gap(m.under_name)
            .child(head)
            .child(content),
    )
}

/// What a notice says (architecture 6.8): "Grace joined", "Ada added Grace".
pub fn notice_words(s: &State, entry: &Entry) -> String {
    let who = s.session.name_of(&entry.author_account_id);
    let about = s.session.name_of(&entry.notice_about);
    match entry.notice.as_str() {
        "joined" => format!("{who} joined"),
        "added" => format!("{who} added {about}"),
        "left" => format!("{who} left"),
        "removed" => format!("{who} removed {about}"),
        _ => String::new(),
    }
}

/// A notice's row: a quiet line with its time, no avatar and nothing to do with it.
fn notice_row(s: &State, entry: &Entry, look: Look) -> El {
    let c = look.c;
    let m = Measures::of(s.settings.density);
    let words = notice_words(s, entry);
    row()
        .key(entry.id.clone())
        .w_full()
        .px(t::SPACE_12)
        .py(t::SPACE_4)
        .gap(m.beside)
        .items_center()
        .role("listitem")
        .label(format!("{words}, {}", clock(entry.at_micros)))
        .child(
            el().w(m.avatar)
                .center()
                .a11y_hidden()
                .child(icon(&icons::MEMBER_JOINED, t::SPACE_14).color(c.ink_faint)),
        )
        .child(
            text(words)
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .truncate()
                .shrink()
                .min_w(0.0),
        )
        .child(
            text(clock(entry.at_micros))
                .font(t::TEXT_META)
                .color(c.ink_faint)
                .a11y_hidden(),
        )
}

fn stored_row(s: &State, i: usize, entry: &Entry, place: Place, look: Look) -> El {
    let c = look.c;
    if !entry.notice.is_empty() {
        return with_divider(s, i, entry, place, notice_row(s, entry, look), look);
    }
    let author = s.session.name_of(&entry.author_account_id);
    let content = if entry.deleted {
        body::note("(deleted)", c.ink_quiet, look)
    } else {
        body::view(&entry.body, true, look)
    };
    let head = Head {
        author: &author,
        author_id: &entry.author_account_id,
        time: clock(entry.at_micros),
        short_time: short_clock(entry.at_micros),
        selectable: true,
    };
    let row = message_frame(s, &entry.id, head, content.key("body"), place, look);
    with_divider(s, i, entry, place, row, look)
}

/// A stored row in its slot. The divider lives in the first row of its day: a row of its
/// own would vanish when the list unmounts it. So does the word that older messages are on
/// their way, in the oldest row.
fn with_divider(s: &State, i: usize, entry: &Entry, place: Place, row: El, look: Look) -> El {
    let c = look.c;
    let day = local(entry.at_micros).date_naive();
    let previous = i
        .checked_sub(1)
        .and_then(|p| s.session.entries.get(p))
        .map(|e| local(e.at_micros).date_naive());
    let m = Measures::of(s.settings.density);
    let mut slot = slot(place, m);
    if i == 0 && s.session.loading_older() {
        slot = slot.child(
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
        slot = slot.child(day_divider(day_label(day), m, look));
    }
    slot.child(row)
}

/// What the list holds a message's row in: the row, with the room under a block's last.
/// The room is the slot's padding and not the row's margin, which a virtual list does not
/// lay out: the 2 points the rows asked for before this never showed.
fn slot(place: Place, m: Measures) -> El {
    col()
        .w_full()
        .pb(if place.closes { m.block_gap } else { 0.0 })
}

/// A send the store does not hold yet: dimmed until it does, with the refusal when there was one.
fn pending_row(s: &State, pending: &Pending, place: Place, look: Look) -> El {
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
    let head = Head {
        author: s.session.self_name(),
        author_id: s.session.me(),
        time,
        short_time: short_clock(pending.sent_at_micros),
        selectable: false,
    };
    let m = Measures::of(s.settings.density);
    slot(place, m).child(message_frame(s, &pending.id, head, content, place, look))
}
