//! A server on screen: its channels in the sidebar, the open channel as the conversation,
//! and its members as the aside, grouped by presence.
//!
//! Not here yet: a new channel and inviting people.

use lumia_client::app::{MemberSummary, ServerSummary};
use rune::{Cursor, El, ScrollSpec, col, el, icon, row, text};

use super::components::{Look, avatar, panel};
use super::conversation::{self, Chrome};
use super::shell::{ASIDE_WIDTH, sidebar_frame};
use crate::app::State;
use crate::session::display_name;
use crate::tokens::{self as t, icons};

/// The server's name and its channels.
pub fn sidebar(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let mut list = col()
        .key("channels")
        .w_full()
        .gap(t::SPACE_2)
        .role("list")
        .label("Channels");
    let mut any = false;
    for channel in Some(server)
        .into_iter()
        .flat_map(|x| s.session.channels_of(&x.server_id))
    {
        any = true;
        let active = s.session.channel() == Some(channel.channel_id.as_str());
        let id = channel.channel_id.clone();
        let mut item = row()
            .key(format!("channel-{}", channel.channel_id))
            .w_full()
            .h(t::ROW_HEIGHT)
            .px(t::SPACE_10)
            .gap(t::SPACE_8)
            .items_center()
            .radius(t::RADIUS_8)
            .role("button")
            .label(format!("{} channel", channel.name))
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| s.select_channel(&id))
            .child(icon(&icons::HASH, t::SPACE_14).color(if active {
                c.accent
            } else {
                c.ink_faint
            }))
            .child(
                text(channel.name.clone())
                    .font(if active { t::TEXT_MEMBER } else { t::TEXT_NAV })
                    .color(if active { c.ink_selected } else { c.ink_quiet })
                    .truncate(),
            );
        if active {
            item = item.fill(c.raised);
            for layer in look.shadows.selected {
                item = item.box_shadow(*layer);
            }
        } else {
            item = item.hover(c.hover_row);
        }
        list = list.child(item);
    }
    let section = text("Channels")
        .font(t::TEXT_SECTION)
        .color(c.ink_muted)
        .px(t::SPACE_10)
        .pt(t::SPACE_4)
        .pb(t::SPACE_6);
    let mut body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(col().w_full().child(section).child(list));
    if !any {
        body = body.child(
            text("No channels yet.")
                .font(t::TEXT_META)
                .color(c.ink_quiet)
                .padding(t::SPACE_10),
        );
    }
    sidebar_frame(s, &server.name, None, body, look)
}

/// The open channel, or what to say when there is none.
pub fn main(s: &State, server: &ServerSummary, look: Look) -> El {
    let Some(channel) = s
        .session
        .channel()
        .and_then(|id| s.session.channel_summary(id))
    else {
        return conversation::vacant(
            &server.name,
            &icons::HASH,
            ("No channel yet", "This server has no channel you can see."),
            look,
        );
    };
    let name = channel.name.as_str();
    conversation::view(
        s,
        Chrome {
            title: name,
            glyph: Some(&icons::HASH),
            list_label: format!("Messages in {name}"),
            placeholder: format!("Message #{name}"),
            empty: ("Nothing here yet", "Say something."),
        },
        look,
    )
}

// ---- Members ----------------------------------------------------------------------------

/// The server's members.
pub fn members(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let mut all: Vec<&MemberSummary> = s
        .session
        .members
        .get(&server.server_id)
        .map(|m| m.iter().collect())
        .unwrap_or_default();
    all.sort_by_cached_key(|m| display_name(m).to_lowercase());
    let (here, away): (Vec<_>, Vec<_>) = all.iter().copied().partition(|m| {
        s.session
            .presence_of(&server.server_id, &m.account_id)
            .is_some()
    });

    let heading = row()
        .w_full()
        .h(t::HEADER_HEIGHT)
        .px(t::SPACE_18)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .child(
            text(format!("Members · {}", all.len()))
                .font(t::TEXT_MEMBER)
                .color(c.ink)
                .role("heading"),
        );
    let section = |label: String| {
        text(label)
            .font(t::TEXT_SECTION)
            .color(c.ink_muted)
            .px(t::SPACE_10)
            .pt(t::SPACE_8)
            .pb(t::SPACE_6)
    };
    let mut list = col()
        .key("members")
        .w_full()
        .px(t::SPACE_8)
        .pb(t::SPACE_14)
        .role("list")
        .label("Members");
    if !here.is_empty() {
        list = list.child(section(format!("Online · {}", here.len())));
        for member in &here {
            list = list.child(member_row(s, server, member, look));
        }
    }
    if !away.is_empty() {
        list = list.child(section(format!(
            "{} · {}",
            if here.is_empty() {
                "Members"
            } else {
                "Offline"
            },
            away.len()
        )));
        for member in &away {
            list = list.child(member_row(s, server, member, look));
        }
    }
    panel("aside", look).w(ASIDE_WIDTH).child(heading).child(
        col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(list),
    )
}

fn member_row(s: &State, server: &ServerSummary, member: &MemberSummary, look: Look) -> El {
    let c = look.c;
    let name = display_name(member);
    let presence = s.session.presence_of(&server.server_id, &member.account_id);
    // The handle and, from the verified profile, the pronouns; then the custom status.
    let handle = if member.handle.is_empty() {
        String::new()
    } else {
        format!("@{}", member.handle)
    };
    let custom = presence.map_or("", |(_, custom)| custom.as_str());
    let subtitle = [handle.as_str(), member.pronouns.as_str(), custom]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let dot = presence.map(|(status, _)| match status.as_str() {
        "AWAY" => c.away,
        "DO_NOT_DISTURB" => c.dnd,
        _ => c.online,
    });
    let mut picture = el().child(avatar(&name, &member.account_id, t::AVATAR_MD, look));
    if let Some(dot) = dot {
        picture = picture.child(
            el().absolute(t::AVATAR_MD - t::SPACE_8, t::AVATAR_MD - t::SPACE_8)
                .size(t::SPACE_10, t::SPACE_10)
                .radius(t::SPACE_10 / 2.0)
                .fill(dot)
                .border(2.0, c.surface),
        );
    }
    let mut words = col().grow(1.0).min_w(0.0).child(
        text(name.clone())
            .font(t::TEXT_MEMBER)
            .color(c.ink)
            .truncate(),
    );
    if !subtitle.is_empty() {
        words = words.child(
            text(subtitle.clone())
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .truncate(),
        );
    }
    let status = match presence.map(|(status, _)| status.as_str()) {
        Some("AWAY") => "away",
        Some("DO_NOT_DISTURB") => "do not disturb",
        Some(_) => "online",
        None => "offline",
    };
    let mut item = row()
        .key(format!("member-{}", member.account_id))
        .w_full()
        .px(t::SPACE_10)
        .py(t::SPACE_6)
        .gap(t::SPACE_10)
        .items_center()
        .radius(t::RADIUS_10)
        .hover(c.hover_member)
        .role("listitem")
        .label(format!("{name}, {status}"))
        .child(picture)
        .child(words);
    if member.account_id == s.session.me() {
        item = item.child(
            el().px(t::SPACE_6)
                .py(t::SPACE_2)
                .radius(t::RADIUS_5)
                .fill(c.accent_soft)
                .child(text("You").font(t::TEXT_TAG).color(c.accent_ink)),
        );
    }
    item
}
