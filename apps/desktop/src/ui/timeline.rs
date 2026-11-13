//! The messages of the conversation on screen: a virtualized list of the session's entries
//! and the sends the store does not hold yet, with a divider above each day's first message.
//! The same list draws a server's channel and a personal conversation.
//!
//! How the rows are drawn is Preferences' (`settings`: layout, spacing, avatars, font size).
//! **Grouped**, the default: messages one person sends close together are one block, as the
//! board's are: the avatar, the name and the time once, over the first, and the rest under
//! it at a paragraph's distance ([`joins`] says what "close together" is). Each message is
//! still a row of its own, so the list unmounts a long block a row at a time; a row after the
//! first has its time in the avatar's column, shown while the pointer is on the row. With
//! avatars hidden there is no such column: the name starts where the avatar was and opens
//! the person's card as the avatar does, and a later row's time is in its label alone.
//! **One line**: every message is `12:04  Priya  words` with no avatar, the time and the
//! name repeated on each, and words that run on wrap under where they start. A reply's
//! reference is a line over the words at that start, and its reactions a row under them.
//! The font size scales every text of a row (and the composer's, `conversation`), never the
//! avatars or the room between.
//!
//! The list is one selection group: a drag runs from one message into the next, Cmd+A in
//! a message takes the conversation, and a copy reads as a transcript. Each block gives it
//! its author and time on one line (a tab between them, as Rune joins texts side by side),
//! each message's body under that (`body` says what a body copies as) with a blank line
//! between two, and a blank line before the next block. Without the names a copied stretch
//! of conversation does not say who said what; a selection inside one body takes neither.
//! One line copies the same: its time and name are drawn before the words, where Rune would
//! join all three with tabs, so they are not selectable text there and the body's first
//! text carries the block's author and time as what a copy writes before it.
//! The day dividers stay out of it: "Today" is wrong by tomorrow. A send still pending is
//! not selectable at all.
//!
//! A message carries what architecture 6.6 adds (`replies`): its quote above its words, its
//! reactions under them, the toolbar while the pointer is on it, a tint and an accent rule
//! when it mentions this account, and a brief one when a jump has just reached it. Above the
//! first message after where this account had read to when the conversation opened stands
//! the "New messages" line. A selection inside one message's body is told to the state, for
//! the "Quote reply" button over it, and the row under the pointer, for the toolbar over it
//! (both drawn at the root, `ui::view`, so that no row carries them).
//!
//! Two limits are Rune's (its issue 550): a copy that spans rows scrolled out of the
//! mounted window builds them to read their text and stops, without saying so, at 10,000
//! such rows or 4 MiB; and while a selection spans many unmounted rows, each scrolled
//! frame that mounts rows costs more (`benches/timeline.rs` measures it).

use std::borrow::Cow;

use chrono::{DateTime, Local, NaiveDate};
use lumia_client::app::QuoteSummary;
use rune::{Cursor, El, GrowthSpec, ScrollSpec, col, el, icon, row, text};

use super::body;
use super::components::{Look, avatar_of};
use super::replies;
use super::scroll::WithScrollBar;
use crate::app::State;
use crate::session::{Entry, Pending};
use crate::settings::{Avatars, Layout, Settings, Spacing};
use crate::tokens::{self as t, icons};

/// How close to the oldest row on screen the reader gets before the page before it is read.
const OLDER_WITHIN: usize = 20;

/// A row's measures, which Preferences' message settings decide. Roomy spacing with large
/// avatars is the boards'; cozy is half the room, and tight less again over and under a
/// block. The avatars' size is a setting of its own, and so is the type's.
#[derive(Clone, Copy)]
struct Measures {
    layout: Layout,
    /// The avatar's size; none when avatars are hidden, and in one line, which has none.
    avatar: Option<f32>,
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
    /// In one line, the room between two lines.
    between: f32,
    ty: t::MessageType,
}

impl Measures {
    fn of(settings: &Settings) -> Self {
        let spacing = match settings.spacing {
            Spacing::Roomy => Self {
                edge: t::SPACE_8,
                block_gap: t::SPACE_8,
                beside: t::SPACE_14,
                under_name: t::SPACE_4,
                divider: t::SPACE_8,
                between: t::SPACE_4,
                ..Self::BASE
            },
            Spacing::Cozy => Self {
                edge: t::SPACE_4,
                block_gap: t::SPACE_4,
                beside: t::SPACE_10,
                under_name: t::SPACE_2,
                divider: t::SPACE_4,
                between: t::SPACE_2,
                ..Self::BASE
            },
            Spacing::Tight => Self {
                edge: t::SPACE_2,
                block_gap: t::SPACE_2,
                beside: t::SPACE_10,
                under_name: t::SPACE_2,
                divider: t::SPACE_4,
                between: 0.0,
                ..Self::BASE
            },
        };
        let avatar = match (settings.layout, settings.avatars) {
            (Layout::OneLine, _) | (_, Avatars::Hidden) => None,
            (Layout::Grouped, Avatars::Large) => Some(t::AVATAR_LG),
            (Layout::Grouped, Avatars::Small) => Some(t::AVATAR_SM),
        };
        Self {
            layout: settings.layout,
            avatar,
            ty: t::MessageType::at(f32::from(settings.font_size.points())),
            ..spacing
        }
    }

    /// What [`Measures::of`] starts each spacing from.
    const BASE: Self = Self {
        layout: Layout::Grouped,
        avatar: None,
        edge: 0.0,
        block_gap: 0.0,
        beside: 0.0,
        under_name: 0.0,
        divider: 0.0,
        between: 0.0,
        ty: t::MESSAGE_TYPE,
    };

    /// A message row's height before it is measured. Grouped, a name over one line of
    /// message, or the avatar where that is taller, with the row's edges and the gap under
    /// it; one line, a line and the room between two.
    fn row_estimate(self) -> f32 {
        let line = self.ty.message.line_height;
        match self.layout {
            Layout::Grouped => {
                let words = line * 2.0 + self.under_name;
                words.max(self.avatar.unwrap_or_default()) + 2.0 * self.edge + self.block_gap
            }
            Layout::OneLine => line + self.between,
        }
    }
}

/// The type of a message's row, and of the composer, at the font size chosen in Preferences.
pub fn type_of(s: &State) -> t::MessageType {
    t::MessageType::at(f32::from(s.settings.font_size.points()))
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
        // The pointer leaving the list leaves no row under it. (Each row says when the
        // pointer comes onto it; a listener per row for leaving cost every frame of a
        // fling a third more.)
        .on_hover_change(|s: &mut State, inside, _| {
            if !inside {
                s.replies.hovered = None;
            }
        })
        // One listener for the whole list: a selection inside one message's body can be
        // quoted. (One on every row cost every selection change a call per row.)
        .on_selection_change(|s: &mut State, change, _| {
            if change.dragging() {
                return;
            }
            let inside = change.selection().and_then(|sel| {
                let mut ids = sel
                    .texts()
                    .iter()
                    .filter(|t| t.is_selected())
                    .map(|t| body_of(t.path()));
                let first = ids.next().flatten()?;
                ids.all(|id| id == Some(first))
                    .then(|| (first.to_string(), sel.text().to_string(), sel.backward()))
            });
            s.selection_in(inside);
        })
        .virtual_rows(
            count,
            Measures::of(&s.settings).row_estimate(),
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
        // Reading up to the oldest rows on screen asks for the page before them; the newest
        // on screen is what marks the conversation read.
        .on_visible_rows(move |s: &mut State, visible| {
            let newest = visible.end >= count;
            let changed = s.replies.newest_visible != newest;
            s.replies.newest_visible = newest;
            (visible.start < OLDER_WITHIN && s.load_older()) || changed
        });
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

/// A message's time, on the clock the system shows: `9:04 AM`, or `09:04`.
fn clock(micros: i64, clock_24: bool) -> String {
    let format = if clock_24 { "%H:%M" } else { "%-I:%M %p" };
    local(micros).format(format).to_string()
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
        // A message that quotes another opens a block of its own, under its name.
        let quotes = |i: usize| match s.session.entries.get(i) {
            Some(entry) => entry.reply.is_some(),
            None => s
                .session
                .pending_here()
                .nth(i - s.session.entries.len())
                .is_some_and(|p| p.reply.is_some()),
        };
        let joined = |i: usize| {
            !quotes(i)
                && i.checked_sub(1)
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

/// The width of the widest time [`clock`] writes in `style`: of every figure in every place,
/// with either half of the day. Measured once for each style, clock and set of fonts.
fn time_width(style: rune::TextStyle, clock_24: bool) -> f32 {
    thread_local! {
        static WIDTH: std::cell::Cell<(u64, f32, bool, f32)> =
            const { std::cell::Cell::new((u64::MAX, 0.0, false, 0.0)) };
    }
    let epoch = rune::text::font_epoch();
    let (measured, size, clock, width) = WIDTH.get();
    if measured == epoch && size == style.size && clock == clock_24 {
        return width;
    }
    let widest = ('0'..='9')
        .flat_map(|d| {
            if clock_24 {
                vec![format!("{d}{d}:{d}{d}")]
            } else {
                vec![format!("1{d}:{d}{d} AM"), format!("1{d}:{d}{d} PM")]
            }
        })
        .map(|probe| rune::text::measure(&probe, &[], style, None).0)
        .fold(0.0, f32::max);
    // Up to the whole point, so a fraction does not set the names between pixels.
    let width = widest.ceil();
    WIDTH.set((epoch, style.size, clock_24, width));
    width
}

/// A time without its half of the day, for the avatar's column: `9:04`, or `09:04`.
fn short_clock(micros: i64, clock_24: bool) -> String {
    let format = if clock_24 { "%H:%M" } else { "%-I:%M" };
    local(micros).format(format).to_string()
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
        .child(text(label).font(m.ty.section).color(c.ink_quiet))
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

/// What a stored message's row has besides its words.
#[derive(Default)]
struct Decor<'a> {
    /// The message, for its toolbar; none for a send on its way.
    id: Option<String>,
    /// It mentions this account, or a jump has just reached it: `accent-wash`.
    tint: bool,
    /// It is the message being replied to: lit as under the pointer while the reply is made.
    lit: bool,
    /// What a reply quotes, for its line of reference.
    quote: Option<&'a QuoteSummary>,
}

/// The message whose body a selected text is in: the row under the list (keyed by the
/// message's id) when the path runs on through its body. None for a name, a time or a quote.
fn body_of(path: &str) -> Option<&str> {
    let mut parts = path.split('/').skip_while(|part| *part != "rows").skip(1);
    let id = parts.next()?;
    parts.any(|part| part == "body").then_some(id)
}

/// The key of a message's row inside its slot in the list.
pub fn frame_key(id: &str) -> String {
    format!("message-{id}")
}

/// What a block's author and time are in a copy, a tab between them, as a grouped block's
/// name and time copy: for the first text of a one-line row's body to carry
/// ([`body::Drawn::head`]).
fn copy_head(author: &str, time: &str) -> String {
    format!("{author}\t{time}")
}

/// The name of a message's author, opening their card when pressed where no avatar does:
/// avatars hidden, or one line. A selectable name inside still takes part in a selection,
/// and a drag on it selects rather than presses.
fn name_button(name: El, author: &str, author_id: &str, key: &str, look: Look) -> El {
    if author_id.is_empty() {
        return name;
    }
    let anchor = format!("author-{key}");
    let (id, at) = (author_id.to_string(), anchor.clone());
    el().key(anchor)
        .role("button")
        .label(format!("{author}, profile"))
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, look.c.focus)
        .on_press(move |s: &mut State| s.open_person(&id, &at))
        .child(name)
}

/// A message's row, as the layout chosen draws it.
fn message_frame(
    s: &State,
    key: &str,
    head: Head<'_>,
    content: El,
    place: Place,
    decor: Decor<'_>,
    look: Look,
) -> El {
    let c = look.c;
    let m = Measures::of(&s.settings);
    let mut frame = col()
        // Its own key, apart from its slot's in the list (which holds a day's divider too): the
        // toolbar over the row under the pointer is anchored to it, and an anchor is found by
        // its key alone.
        .key(frame_key(key))
        .w_full()
        .px(t::SPACE_12)
        .hover(c.hover_message)
        .role("listitem");
    if decor.tint {
        frame = frame.fill(c.accent_wash);
    } else if decor.lit {
        frame = frame.fill(c.hover_message);
    }
    if let Some(id) = decor.id {
        // The toolbar over the row under the pointer is drawn at the root (`ui::view`), so a
        // row builds none of it; the row only says when the pointer comes onto it.
        frame = frame.on_hover(move |s: &mut State| s.hovered(&id, true));
    }
    let row = Row {
        frame,
        key,
        head,
        content,
        quote: decor.quote,
        m,
    };
    match m.layout {
        Layout::Grouped => grouped_frame(s, row, place, look),
        Layout::OneLine => line_frame(s, row, look),
    }
}

/// What both layouts draw a message's row from.
struct Row<'a> {
    /// The row's frame, with its fill and its hover.
    frame: El,
    key: &'a str,
    head: Head<'a>,
    content: El,
    quote: Option<&'a QuoteSummary>,
    m: Measures,
}

/// A grouped row. The first of a block is an avatar beside a name, a time and the message;
/// the ones after it are the message alone under those, in the same column.
fn grouped_frame(s: &State, row_: Row<'_>, place: Place, look: Look) -> El {
    let c = look.c;
    let Row {
        frame,
        key,
        head,
        content,
        quote,
        m,
    } = row_;
    let Head {
        author,
        author_id,
        time,
        short_time,
        selectable,
    } = head;
    // A block's rows touch, and are a paragraph's distance apart between them: the board
    // draws a block's messages as the paragraphs of one.
    let within = m.under_name / 2.0;
    let mut frame = frame
        .pt(if place.opens { m.edge } else { within })
        .pb(if place.closes { m.edge } else { within })
        .gap(t::SPACE_2)
        .radius(t::RADIUS_12);
    if let Some(quote) = quote {
        // The connector starts under the avatar's centre, or at the row's start with none.
        let indent = m.avatar.map_or(0.0, |avatar| avatar / 2.0 - 1.0);
        frame = frame.child(replies::reference_line(s, quote, indent, &m.ty, look));
    }
    if !place.opens {
        let content = col().grow(1.0).min_w(0.0).child(content);
        let Some(avatar) = m.avatar else {
            // No avatar's column to show the time in: who and when are the row's label.
            return frame.label(format!("{author}, {time}")).child(content);
        };
        // The time of a message under the block's first, where the avatar would be: there
        // for assistive technology always, and for the eye while the pointer is on the row.
        let when = el().w(avatar).h(m.ty.message.line_height).center().child(
            text(short_time)
                .key("time")
                .font(m.ty.meta)
                .color(c.ink_muted)
                .label(format!("{author}, {time}"))
                .group_opacity(0.0, 1.0),
        );
        return frame
            .hover_group()
            .child(row().w_full().gap(m.beside).child(when).child(content));
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
    let name = part(text(author).key("author").font(m.ty.author).color(c.ink), 2);
    let time = part(text(time).key("time").font(m.ty.meta).color(c.ink_muted), 1);
    let Some(avatar) = m.avatar else {
        // No avatar to open the person's card, so the name does.
        let head = row()
            .gap(t::SPACE_8)
            .items_center()
            .child(name_button(name, author, author_id, key, look))
            .child(time);
        return frame.child(col().w_full().gap(m.under_name).child(head).child(content));
    };
    // The avatar opens its person's card, as the board's does. The name stays text: it is
    // part of what a selection copies, and a press on it starts one.
    let mut picture = el().child(avatar_of(s, author, author_id, avatar, look));
    if !author_id.is_empty() {
        let anchor = format!("avatar-{key}");
        let (id, at) = (author_id.to_string(), anchor.clone());
        picture = picture
            .key(anchor)
            .radius(avatar / 2.0)
            .role("button")
            .label(format!("{author}, profile"))
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(move |s: &mut State| s.open_person(&id, &at));
    }
    let head = row().gap(t::SPACE_8).items_center().child(name).child(time);
    frame.child(
        row().w_full().gap(m.beside).child(picture).child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(m.under_name)
                .child(head)
                .child(content),
        ),
    )
}

/// A one-line row: the time and the name, then the words, which wrap under their own start.
/// A reply's line of reference is over the words at that start: Rune lays a row's children
/// out side by side, so the line is the first thing in the words' column, and the column of
/// the time and the name starts with a space as tall as it, which keeps the two level with
/// the words' first line.
fn line_frame(s: &State, row_: Row<'_>, look: Look) -> El {
    let c = look.c;
    let Row {
        frame,
        key,
        head,
        content,
        quote,
        m,
    } = row_;
    // The time and the name each stand in the height of the words' first line, centred in
    // it, so the three read as one line whatever their sizes. The time takes as much room
    // as the widest time takes, so the names after the times stand in a column.
    let level = |part: El| el().h(m.ty.message.line_height).center().child(part);
    let time = row()
        .h(m.ty.message.line_height)
        .min_w(time_width(m.ty.meta, s.clock_24))
        .items_center()
        .child(
            text(head.time)
                .key("time")
                .font(m.ty.meta)
                .color(c.ink_muted),
        );
    let name = level(
        text(head.author)
            .key("author")
            .font(m.ty.author)
            .color(c.ink),
    );
    let name = name_button(name, head.author, head.author_id, key, look);
    let mut lead = col().gap(t::SPACE_2);
    let mut words = col().grow(1.0).min_w(0.0).gap(t::SPACE_2);
    if let Some(quote) = quote {
        let height = m.ty.reference.line_height;
        lead = lead.child(el().h(height));
        words = words.child(replies::reference_short(s, quote, height, &m.ty, look));
    }
    let lead = lead.child(row().gap(t::SPACE_8).child(time).child(name));
    frame.py(m.between / 2.0).radius(t::RADIUS_8).child(
        row()
            .w_full()
            .gap(t::SPACE_8)
            .child(lead)
            .child(words.child(content)),
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

/// A notice's row: a quiet line with its time, no avatar and nothing to do with it. In one
/// line the time comes first, as a message's does.
fn notice_row(s: &State, entry: &Entry, look: Look) -> El {
    let c = look.c;
    let m = Measures::of(&s.settings);
    let words = notice_words(s, entry);
    let when = clock(entry.at_micros, s.clock_24);
    let line = row()
        .key(entry.id.clone())
        .w_full()
        .px(t::SPACE_12)
        .items_center()
        .role("listitem")
        .label(format!("{words}, {when}"));
    let glyph = el()
        .center()
        .a11y_hidden()
        .child(icon(&icons::MEMBER_JOINED, t::SPACE_14).color(c.ink_faint));
    let words = text(words)
        .font(m.ty.meta)
        .color(c.ink_muted)
        .truncate()
        .shrink()
        .min_w(0.0);
    let when = text(when).font(m.ty.meta).a11y_hidden();
    match (m.layout, m.avatar) {
        (Layout::OneLine, _) => line
            .min_h(m.ty.message.line_height)
            .py(m.between / 2.0)
            .gap(t::SPACE_8)
            // In the times' column, so the glyph stands where the names do.
            .child(
                when.color(c.ink_muted)
                    .min_w(time_width(m.ty.meta, s.clock_24)),
            )
            .child(glyph)
            .child(words),
        (Layout::Grouped, avatar) => line
            .py(t::SPACE_4)
            .gap(if avatar.is_some() {
                m.beside
            } else {
                t::SPACE_8
            })
            // The glyph in the avatars' column, or where the name starts with none.
            .child(match avatar {
                Some(avatar) => glyph.w(avatar),
                None => glyph,
            })
            .child(words)
            .child(when.color(c.ink_faint)),
    }
}

fn stored_row(s: &State, i: usize, entry: &Entry, place: Place, look: Look) -> El {
    let c = look.c;
    if !entry.notice.is_empty() {
        return with_divider(s, i, entry, place, notice_row(s, entry, look), look);
    }
    let m = Measures::of(&s.settings);
    let author = s.session.name_of(&entry.author_account_id);
    let time = clock(entry.at_micros, s.clock_24);
    // One line draws the block's author and time where a selection does not take them; its
    // first message's body carries them for a copy instead.
    let head = (m.layout == Layout::OneLine && place.opens).then(|| copy_head(&author, &time));
    let drawn = body::Drawn {
        ty: &m.ty,
        head: head.as_deref(),
    };
    let content = if entry.deleted {
        body::note("(deleted)", c.ink_quiet, drawn, look).key("body")
    } else {
        message_content(s, entry, drawn, look)
    };
    let head = Head {
        author: &author,
        author_id: &entry.author_account_id,
        time,
        short_time: short_clock(entry.at_micros, s.clock_24),
        selectable: true,
    };
    let decor = Decor {
        id: (!entry.deleted).then(|| entry.id.clone()),
        tint: entry.mentions_me || s.flashed(&entry.id),
        lit: s
            .replies
            .replying
            .as_ref()
            .is_some_and(|q| q.message_id == entry.id),
        quote: entry.reply.as_ref(),
    };
    let row = message_frame(s, &entry.id, head, content, place, decor, look);
    with_divider(s, i, entry, place, row, look)
}

/// What a message's body shows: its source with each mention's words in it as `@` and the
/// person's name (architecture 6.6; `@everyone` as it is), escaped so the name stays text, and
/// those words, for the renderer to draw as pills.
fn shown_body(s: &State, entry: &Entry) -> (String, Vec<String>) {
    let mut mentions: Vec<_> = entry
        .mentions
        .iter()
        .filter(|m| {
            entry.body.is_char_boundary(m.start as usize)
                && entry.body.is_char_boundary(m.end as usize)
        })
        .filter(|m| m.start < m.end && (m.end as usize) <= entry.body.len())
        .collect();
    mentions.sort_by_key(|m| m.start);
    let mut source = String::with_capacity(entry.body.len());
    let mut words = Vec::new();
    let mut at = 0usize;
    for m in mentions {
        let (start, end) = (m.start as usize, m.end as usize);
        if start < at {
            continue;
        }
        source.push_str(&entry.body[at..start]);
        let shown = if m.everyone {
            "@everyone".to_string()
        } else {
            format!("@{}", s.session.name_of(&m.account_id))
        };
        for ch in shown.chars() {
            if "\\`*_[]~#>|<".contains(ch) {
                source.push('\\');
            }
            source.push(ch);
        }
        words.push(shown);
        at = end;
    }
    source.push_str(&entry.body[at..]);
    (source, words)
}

/// A message's words with what architecture 6.6 adds: its mentions as pills in them,
/// "(edited)" after them, and its reactions under them; in their place the field while this
/// account edits it, and under them why an edit or a delete of it was refused.
fn message_content(s: &State, entry: &Entry, drawn: body::Drawn<'_>, look: Look) -> El {
    let c = look.c;
    let ty = drawn.ty;
    let editing = s
        .replies
        .editing
        .as_ref()
        .filter(|(id, _)| *id == entry.id)
        .map(|(_, field)| field);
    let mut content = col().w_full().gap(t::SPACE_4);
    match editing {
        Some(field) => content = content.child(edit_field(&entry.id, field, ty, look)),
        None => {
            let (source, words) = shown_body(s, entry);
            let body = body::view_with(&source, true, &words, drawn, look);
            content = content.child(body.key("body"));
            if entry.edited {
                content = content.child(
                    text("(edited)")
                        .key("edited")
                        .font(ty.meta)
                        .color(c.ink_muted)
                        .label("edited"),
                );
            }
        }
    }
    if let Some((_, why)) = s
        .session
        .reads
        .refused
        .as_ref()
        .filter(|(id, _)| *id == entry.id)
    {
        content = content.child(
            text(format!("Not saved: {why}"))
                .font(ty.meta)
                .color(c.dnd)
                .role("status"),
        );
    }
    if !entry.reactions.is_empty() {
        content = content.child(replies::reactions(s, entry, ty, look).mt(t::SPACE_2));
    }
    content
}

/// A message being edited in place: the field with its body, and how to leave it.
fn edit_field(id: &str, field: &rune::TextEdit, ty: &t::MessageType, look: Look) -> El {
    let c = look.c;
    col()
        .w_full()
        .gap(t::SPACE_6)
        .child(
            col()
                .w_full()
                .px(t::SPACE_12)
                .py(t::SPACE_8)
                .radius(t::RADIUS_12)
                .fill(c.field)
                .border(1.0, c.border_active)
                .child(
                    rune::text_input(field, ty.message)
                        .key(format!("edit-{id}"))
                        .label("Edit message")
                        .color(c.ink_field)
                        .caret(c.accent, c.accent_soft)
                        .w_full()
                        .min_h(ty.message.line_height)
                        .max_h(ty.message.line_height * 8.0)
                        .on_edit(State::edit_field)
                        .on_input_key(|s: &mut State, key, tx| s.edit_key(key, tx)),
                ),
        )
        .child(
            text("Esc to cancel · Return to save")
                .font(ty.meta)
                .color(c.ink_muted),
        )
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
    let m = Measures::of(&s.settings);
    let mut slot = slot(place, m);
    if i == 0 && s.session.loading_older() {
        slot = slot.child(
            text("Loading earlier messages…")
                .font(m.ty.meta)
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
    if s.session.new_line_at() == Some(entry.id.as_str()) {
        slot = slot.child(new_line(m, look));
    }
    slot.child(row)
}

/// The "New messages" line, above the first message after where this account had read to.
fn new_line(m: Measures, look: Look) -> El {
    let c = look.c;
    // The board's: a rule in `accent-line`, then "New"; nothing after the word.
    row()
        .key("new-messages")
        .w_full()
        .px(t::SPACE_10)
        .py(m.divider.min(t::SPACE_6))
        .gap(t::SPACE_12)
        .items_center()
        .role("separator")
        .label("New messages")
        .child(el().grow(1.0).h(1.0).fill(c.accent_line))
        .child(
            text("New")
                .font(m.ty.section_strong)
                .color(c.accent_ink)
                .a11y_hidden(),
        )
}

/// What the list holds a message's row in: the row, with the room under a block's last.
/// The room is the slot's padding and not the row's margin, which a virtual list does not
/// lay out: the 2 points the rows asked for before this never showed. One line has no
/// blocks to put room between; its rows keep the room between lines themselves.
fn slot(place: Place, m: Measures) -> El {
    let grouped = m.layout == Layout::Grouped;
    col().w_full().pb(if place.closes && grouped {
        m.block_gap
    } else {
        0.0
    })
}

/// A send the store does not hold yet: dimmed until it does, with the refusal when there was one.
fn pending_row(s: &State, pending: &Pending, place: Place, look: Look) -> El {
    let c = look.c;
    let m = Measures::of(&s.settings);
    let drawn = body::Drawn {
        ty: &m.ty,
        head: None,
    };
    let body = body::view_with(&pending.body, false, &[], drawn, look).key("body");
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
                .child(text("Dismiss").font(m.ty.link_sm).color(c.link));
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
                                .font(m.ty.meta)
                                .color(c.dnd),
                        )
                        .child(dismiss),
                )
        }
    };
    let time = if pending.failed.is_some() {
        clock(pending.sent_at_micros, s.clock_24)
    } else {
        "Sending…".into()
    };
    let head = Head {
        author: s.session.self_name(),
        author_id: s.session.me(),
        time,
        short_time: short_clock(pending.sent_at_micros, s.clock_24),
        selectable: false,
    };
    slot(place, m).child(message_frame(
        s,
        &pending.id,
        head,
        content,
        place,
        Decor {
            quote: pending.reply.as_ref(),
            ..Decor::default()
        },
        look,
    ))
}
