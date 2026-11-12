//! The personal conversation on screen, and who is in it as the aside.
//!
//! What the log says besides its messages is said above them: that the conversation is
//! end-to-end encrypted and for whom, that this device is not in its group yet (with the
//! way to rejoin), how many messages were sent before this device joined, and how many it
//! could not decrypt.
//!
//! Not here: editing and deleting a message, and the state of history backup (nothing in the
//! core can say from here whether it is on).

use lumia_client::app::ChannelSummary;
use rune::{El, Icon, ScrollSpec, col, el, icon, row, text};

use super::{action_button, at, no_identity, person};
use crate::app::{HomeView, State};
use crate::tokens::{self as t, icons};
use crate::ui::components::{
    Look, empty_state, error_text, notice, panel, section_label, small_button, tag,
};
use crate::ui::conversation::{self, Chrome, Extras};
use crate::ui::scroll::WithScrollBar;
use crate::ui::shell::ASIDE_WIDTH;

/// The conversation on screen, when the snapshot lists it.
fn open(s: &State) -> Option<&ChannelSummary> {
    s.session.dm().and_then(|id| s.session.channel_summary(id))
}

pub fn view(s: &State, look: Look) -> El {
    if !s.session.has_identity() {
        return no_identity(look);
    }
    let Some(channel) = open(s) else {
        return choose(look);
    };
    let title = s.session.conversation_name(channel);
    let others: Vec<&str> = s.session.others_in(channel).collect();
    let member = s.session.dm_member(&channel.channel_id);
    let log = s.session.dm_log(&channel.channel_id);

    // The other side's pronouns, from their verified profile, then the host.
    let pronouns = others
        .iter()
        .filter_map(|id| s.session.person(id))
        .map(|p| p.pronouns.as_str())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let host = if channel.host.is_empty() {
        String::new()
    } else {
        format!("on {}", channel.host)
    };
    let topic = [pronouns, host]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");

    let readers = if others.is_empty() {
        "the other participants".to_string()
    } else {
        others
            .iter()
            .map(|id| s.session.name_of(id))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut notices = vec![notice(
        &icons::SHIELD,
        format!(
            "Messages here are end-to-end encrypted. Only your devices and those of {readers} \
             can read them."
        ),
        look,
    )];
    if member == Some(false) {
        notices.push(rejoin_notice(s, look));
    }
    if let Some(log) = log {
        if log.before_join > 0 {
            notices.push(notice(
                &icons::LOCK,
                format!(
                    "{} earlier {} sent before this device joined and cannot be read here.",
                    log.before_join,
                    if log.before_join == 1 {
                        "message was"
                    } else {
                        "messages were"
                    }
                ),
                look,
            ));
        }
        if log.undecryptable > 0 {
            notices.push(notice(
                &icons::LOCK,
                format!("{} could not be decrypted.", log.undecryptable),
                look,
            ));
        }
    }

    let writable = member == Some(true);
    conversation::view_with(
        s,
        Chrome {
            title: &title,
            glyph: None,
            list_label: format!("Messages with {title}"),
            placeholder: if writable {
                format!("Message {title}")
            } else {
                "Setting up the encrypted group…".into()
            },
            empty: if writable {
                ("Nothing here yet", "Say something.")
            } else {
                (
                    "Nothing to read yet",
                    "Messages show once this device is in the conversation's group.",
                )
            },
        },
        Extras {
            topic: Some(topic),
            notices,
            empty_glyph: Some(&icons::LOCK),
            composer_disabled: !writable,
        },
        look,
    )
}

/// Home with no conversation chosen.
fn choose(look: Look) -> El {
    panel("panel", look)
        .w_full()
        .child(conversation::header("Home", None, look))
        .child(
            empty_state(
                &icons::LOCK,
                "Your conversations",
                "Personal messages are end-to-end encrypted. Pick one on the left, or start one \
                 with someone you share a server with.",
                look,
            )
            .key("choose-conversation")
            .child(el().pt(t::SPACE_8).child(action_button(
                "start-new-message",
                "New message",
                true,
                |s| s.go_home(HomeView::NewMessage),
                look,
            ))),
        )
}

/// This device holds no group for the conversation: why, and the way back in.
fn rejoin_notice(s: &State, look: Look) -> El {
    let c = look.c;
    let mut words = col().grow(1.0).min_w(0.0).gap(t::SPACE_4).child(
        text(
            "This device is not in the conversation's group yet. The device that created it \
             sends a welcome; if that never comes, rejoin from the instance's group info.",
        )
        .font(t::TEXT_COMPACT)
        .color(c.accent_ink)
        .wrap(),
    );
    if !s.home.rejoin_error.is_empty() {
        words = words.child(error_text(&s.home.rejoin_error, look));
    }
    accent_notice(&icons::LOCK, words, look).child(small_button(
        "rejoin",
        if s.home.rejoining {
            "Rejoining…"
        } else {
            "Rejoin"
        },
        !s.home.rejoining,
        |s, _| s.rejoin_dm(),
        look,
    ))
}

/// A notice that asks for something: the accent's tint, with room for a button at its end.
fn accent_notice(glyph: &'static Icon, words: El, look: Look) -> El {
    let c = look.c;
    row()
        .key("rejoin-notice")
        .w_full()
        .px(t::SPACE_14)
        .py(t::SPACE_10)
        .gap(t::SPACE_10)
        .items_center()
        .radius(t::RADIUS_12)
        .fill(c.accent_soft)
        .child(icon(glyph, t::SPACE_16).color(c.accent_ink))
        .child(words)
}

/// Who is in the conversation on screen, as the aside panel.
pub fn aside(s: &State, look: Look) -> Option<El> {
    if !s.session.has_identity() {
        return None;
    }
    let channel = open(s)?;
    let c = look.c;
    let heading = row()
        .w_full()
        .h(t::HEADER_HEIGHT)
        .px(t::SPACE_18)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .child(
            text("Details")
                .font(t::TEXT_TAB)
                .color(c.ink)
                .role("heading"),
        );
    let mut list = col()
        .key("participants")
        .w_full()
        .role("list")
        .label("In this conversation");
    for id in &channel.participant_account_ids {
        list = list.child(participant(s, id, look));
    }
    Some(
        panel("aside", look).w(ASIDE_WIDTH).child(heading).child(
            col()
                .key("participants")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(
                    col()
                        .w_full()
                        .px(t::SPACE_8)
                        .pt(t::SPACE_4)
                        .pb(t::SPACE_14)
                        .child(section_label("In this conversation", t::SPACE_8, look))
                        .child(list),
                )
                .with_scroll_bar(look),
        ),
    )
}

/// Someone in the conversation: their handle and, from the verified profile, their pronouns
/// and bio.
fn participant(s: &State, account_id: &str, look: Look) -> El {
    let c = look.c;
    let name = s.session.name_of(account_id);
    let me = account_id == s.session.me();
    let known = s.session.person(account_id);
    let handle = match (known, me) {
        (Some(person), _) => at(&person.handle),
        (None, true) => at(s
            .session
            .snapshot
            .as_ref()
            .map_or("", |snap| snap.handle.as_str())),
        (None, false) => String::new(),
    };
    let pronouns = known.map_or("", |p| p.pronouns.as_str());
    let subtitle = [handle.as_str(), pronouns]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let top = row().w_full().items_center().child(person(
        s,
        &name,
        &subtitle,
        account_id,
        t::AVATAR_MD,
        me.then(|| tag("You", look)),
        look,
    ));
    let mut item = col()
        .key(format!("participant-{account_id}"))
        .w_full()
        .px(t::SPACE_10)
        .py(t::SPACE_6)
        .mb(t::SPACE_2)
        .gap(t::SPACE_6)
        .radius(t::RADIUS_10)
        .hover(c.hover_member)
        .role("listitem")
        .label(if me { format!("{name}, you") } else { name })
        .child(top);
    if let Some(bio) = known.map(|p| p.bio.as_str()).filter(|bio| !bio.is_empty()) {
        item = item.child(
            text(bio)
                .font(t::TEXT_META)
                .color(c.ink_secondary)
                .wrap()
                .w_full(),
        );
    }
    item
}
