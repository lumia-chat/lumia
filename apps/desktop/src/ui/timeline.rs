//! The messages of the conversation on screen: a virtualized list of the session's entries
//! and the sends the store does not hold yet, with a divider above each day's first message.
//! The same list draws a server's channel and a personal conversation.

use std::borrow::Cow;

use chrono::{DateTime, Local, NaiveDate};
use rune::{Cursor, El, GrowthSpec, ScrollSpec, Tx, col, el, row, text};
use rune_kit::{Markdown, MarkdownStyle};

use super::components::{Look, avatar};
use crate::app::State;
use crate::session::{Entry, Pending};
use crate::tokens::{self as t};

/// A message row's height before it is measured: an avatar and its padding.
const ROW_ESTIMATE: f32 = t::AVATAR_LG + t::SPACE_24;

thread_local! {
    /// Message-body styles for light and dark, built once.
    static STYLES: [MarkdownStyle; 2] = [t::markdown(false), t::markdown(true)];
}

/// The list, pinned to its newest message. `label` names it for assistive technology.
/// Call it only when there is something to list.
pub fn view(s: &State, label: String, look: Look) -> El {
    let count = s.session.entries.len() + s.session.pending_here().count();
    let dark = look.dark;
    let rows = col()
        .key("rows")
        .w_full()
        .py(t::SPACE_14)
        .px(t::SPACE_14)
        .growth(GrowthSpec::default())
        .role("list")
        .label(label)
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
        .virtual_rows_version(s.session.revision);
    col()
        .key("messages")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        .child(rows)
}

/// Whether the open conversation has anything to list.
pub fn is_empty(s: &State) -> bool {
    s.session.entries.is_empty() && s.session.pending_here().next().is_none()
}

/// A message body: the protocol's Markdown subset. `selectable` is off for a send that is
/// still pending.
fn body(source: &str, selectable: bool, look: Look) -> El {
    STYLES.with(|styles| {
        let markdown = Markdown::new(source).selectable(selectable);
        let markdown = if selectable {
            markdown.on_link_tx(open_link)
        } else {
            markdown
        };
        markdown.render(&styles[usize::from(look.dark)]).key("body")
    })
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
        .py(t::SPACE_8)
        .gap(t::SPACE_12)
        .items_center()
        .child(rule())
        .child(text(label).font(t::TEXT_SECTION).color(c.ink_quiet))
        .child(rule())
}

/// An avatar beside a name, a time, and whatever the message is.
fn message_frame(
    key: &str,
    author: &str,
    author_id: &str,
    time: String,
    content: El,
    look: Look,
) -> El {
    let c = look.c;
    let head = row()
        .gap(t::SPACE_8)
        .items_center()
        .child(text(author).font(t::TEXT_AUTHOR).color(c.ink_emphasis))
        .child(text(time).font(t::TEXT_META).color(c.ink_muted));
    row()
        .key(key)
        .w_full()
        .px(t::SPACE_12)
        .py(t::SPACE_8)
        .gap(t::SPACE_14)
        .radius(t::RADIUS_12)
        .hover(c.hover_message)
        .role("listitem")
        .child(avatar(author, author_id, t::AVATAR_LG, look))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(t::SPACE_2)
                .child(head)
                .child(content),
        )
}

/// Opens a link from a message in the browser. Only web links: a message is text someone
/// else wrote, and other schemes can start other programs.
fn open_link(_: &mut State, url: &str, tx: &mut Tx) {
    if url.starts_with("https://") || url.starts_with("http://") {
        let _ = tx.open_url(url);
    }
}

fn stored_row(s: &State, i: usize, entry: &Entry, look: Look) -> El {
    let c = look.c;
    let author = s.session.name_of(&entry.author_account_id);
    let content = if entry.deleted {
        text("(deleted)")
            .font(t::TEXT_MESSAGE)
            .color(c.ink_quiet)
            .key("body")
    } else {
        body(&entry.body, true, look)
    };
    let row = message_frame(
        &entry.id,
        &author,
        &entry.author_account_id,
        clock(entry.at_micros),
        content,
        look,
    );
    // The divider lives in the first row of its day: a row of its own would vanish when the
    // list unmounts it.
    let day = local(entry.at_micros).date_naive();
    let previous = i
        .checked_sub(1)
        .and_then(|p| s.session.entries.get(p))
        .map(|e| local(e.at_micros).date_naive());
    if previous == Some(day) {
        return row;
    }
    col()
        .key(format!("day-{}", entry.id))
        .w_full()
        .child(day_divider(day_label(day), look))
        .child(row)
}

/// A send the store does not hold yet: dimmed until it does, with the refusal when there was one.
fn pending_row(s: &State, pending: &Pending, look: Look) -> El {
    let c = look.c;
    let body = body(&pending.body, false, look);
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
        look,
    )
}
