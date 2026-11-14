//! Mentions (architecture 6.6): the messages that notify this account (they mention it or
//! everyone, or quote it), newest first, in a server's main pane for that server and at Home
//! for every server. Each says where it was said over the message as the timeline draws it;
//! pressing one opens its channel at it. Older ones are read as the list nears its end.

use std::borrow::Cow;

use lumia_client::app::MessageSummary;
use rune::{Cursor, El, GrowthSpec, ScrollSpec, col, el, row, text};

use super::body;
use super::components::{Look, avatar_of, empty_state, error_text, panel};
use super::conversation::header_with;
use super::replies;
use super::scroll::WithScrollBar;
use crate::app::State;
use crate::session::Entry;
use crate::session::reads::MentionsPage;
use crate::tokens::{self as t, icons};

/// How close to the end of what is listed the reader gets before older ones are read.
const MORE_WITHIN: usize = 5;

/// The Mentions page of a server, or of every server for `None`.
pub fn view(s: &State, server_id: Option<&str>, look: Look) -> El {
    let c = look.c;
    let page = s.session.mentions_page(server_id);
    let count = page.map_or(0, |p| p.messages.len());
    let header = header_with(
        "Mentions",
        Some(&icons::MENTION),
        Some(match server_id {
            Some(_) => "Messages here that mention you or reply to you",
            None => "Messages in every server that mention you or reply to you",
        }),
        look,
    );
    let mut pane = panel("panel", look).w_full().child(header);
    if let Some(error) = page.and_then(|p| p.error.as_deref()) {
        pane = pane.child(
            el().w_full()
                .px(t::SPACE_24)
                .pt(t::SPACE_12)
                .child(error_text(error, look)),
        );
    }
    if count == 0 {
        let loading = page.is_none_or(|p| p.loading);
        return pane.child(if loading {
            el().w_full().grow(1.0).center().child(
                text("Loading…")
                    .font(t::TEXT_META)
                    .color(c.ink_muted)
                    .role("status"),
            )
        } else {
            empty_state(
                &icons::MENTION,
                "No mentions yet",
                "When someone mentions you or replies to you, it shows up here.",
                look,
            )
        });
    }
    let server = server_id.map(str::to_string);
    let rows = col()
        .key("mention-rows")
        .w_full()
        .px(t::SPACE_24)
        .py(t::SPACE_14)
        .growth(GrowthSpec::default())
        .role("list")
        .label("Mentions")
        .virtual_rows(
            count,
            t::AVATAR_LG * 3.0,
            |s: &State, i| {
                shown(s)
                    .and_then(|p| p.messages.get(i))
                    .map_or(Cow::Borrowed(""), |m| {
                        Cow::Owned(format!("mention-{}", m.message_id))
                    })
            },
            move |s: &State, i| match shown(s).and_then(|p| p.messages.get(i)) {
                Some(message) => item(s, message, Look::of(look.dark)),
                None => el(),
            },
        )
        .on_visible_rows(move |s: &mut State, visible| {
            let wants = visible.end + MORE_WITHIN >= count;
            if wants {
                s.session.load_mentions(server.as_deref(), true);
            }
            false
        });
    pane.child(
        col()
            .key("mentions")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(rows)
            .with_scroll_bar("mentions", look),
    )
}

/// The list the page on screen shows: the server's, or every server's at Home.
fn shown(s: &State) -> Option<&MentionsPage> {
    s.session.mentions_page(s.server_id())
}

/// Where a mention was said: `#design`, or at Home `Acme · #design`.
fn place_of(s: &State, message: &MessageSummary) -> String {
    let Some(channel) = s.session.channel_summary(&message.channel_id) else {
        return String::new();
    };
    let name = format!("#{}", channel.name);
    if s.server_id().is_some() {
        return name;
    }
    match s
        .session
        .servers()
        .iter()
        .find(|x| x.server_id == channel.server_id)
    {
        Some(server) => format!("{} · {name}", server.name),
        None => name,
    }
}

/// One mention: where, then who and when, and the message with its quote.
fn item(s: &State, message: &MessageSummary, look: Look) -> El {
    let c = look.c;
    let entry = Entry::from(message.clone());
    let author = s.session.name_of(&entry.author_account_id);
    let at = chrono::DateTime::from_timestamp_micros(entry.at_micros)
        .unwrap_or_default()
        .with_timezone(&chrono::Local)
        .format("%b %-d, %-I:%M %p")
        .to_string();
    let words: Vec<String> = entry
        .mentions
        .iter()
        .filter_map(|m| entry.body.get(m.start as usize..m.end as usize))
        .map(str::to_string)
        .collect();
    let place = place_of(s, message);
    let mut content = col().grow(1.0).min_w(0.0).gap(t::SPACE_4).child(
        row()
            .gap(t::SPACE_8)
            .items_center()
            .child(text(author.clone()).font(t::TEXT_AUTHOR).color(c.ink))
            .child(text(at.clone()).font(t::TEXT_META).color(c.ink_muted)),
    );
    if let Some(quote) = &entry.reply {
        content = content.child(replies::reference_line(
            s,
            quote,
            0.0,
            &t::MESSAGE_TYPE,
            look,
        ));
    }
    content = content.child(body::view_with(
        &entry.body,
        false,
        &words,
        body::Drawn::default(),
        look,
    ));
    let (channel_id, message_id) = (message.channel_id.clone(), message.message_id.clone());
    col()
        .key(format!("mention-{}", message.message_id))
        .w_full()
        .mb(t::SPACE_8)
        .padding(t::SPACE_12)
        .gap(t::SPACE_8)
        .radius(t::RADIUS_12)
        .fill(c.inset)
        .hover(c.hover_message)
        .role("listitem")
        .label(format!("{author} in {place}, {at}: {}", entry.body))
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press_tx(move |s: &mut State, tx| s.open_mention(&channel_id, &message_id, tx))
        .child(text(place).font(t::TEXT_SECTION).color(c.ink_muted))
        .child(
            row()
                .w_full()
                .gap(t::SPACE_12)
                .child(avatar_of(
                    s,
                    &author,
                    &entry.author_account_id,
                    t::AVATAR_MD,
                    look,
                ))
                .child(content),
        )
}
