//! A server on screen: its channels in the sidebar, the open channel as the conversation,
//! and its members as the aside, grouped by presence. The name opens the server's menu
//! (`menus::server`), where "Invite people" and "New channel" are.

use lumia_client::app::{MemberSummary, ServerSummary};
use rune::{Cursor, El, ScrollSpec, col, el, icon, row, text, text_input};

use super::components::{Look, avatar_of, icon_button, panel, section_label, status_dot, tag};
use super::conversation::{self, Chrome};
use super::shell::{ASIDE_WIDTH, sidebar_frame};
use crate::app::{HomeView, State};
use crate::session::display_name;
use crate::tokens::{self as t, icons};
use crate::ui::scroll::WithScrollBar;

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
        // The row's own type: 700 on the open channel. (The board's 600 is for a channel
        // with unread messages, which the session does not count.)
        let style = if active {
            t::TEXT_NAV_ACTIVE
        } else {
            t::TEXT_NAV
        };
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
            // The hash is a character of the row's type in a column of its own, as the
            // board sets it.
            .child(
                text("#")
                    .font(style)
                    .color(if active { c.accent } else { c.ink_faint })
                    .w(t::SPACE_10)
                    .a11y_hidden(),
            )
            .child(
                text(channel.name.clone())
                    .font(style)
                    .color(if active { c.ink_selected } else { c.ink_quiet })
                    .truncate()
                    .grow(1.0)
                    .min_w(0.0),
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
    let mut channels = col()
        .w_full()
        .child(section_label("Channels", t::SPACE_4, look))
        .child(list);
    if !any {
        channels = channels.child(
            text("No channels yet. Add one from the menu under the server’s name.")
                .font(t::TEXT_META)
                .color(c.ink_quiet)
                .wrap()
                .padding(t::SPACE_10),
        );
    }
    let body = col()
        .key("channels")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(channels)
        .with_scroll_bar(look);
    // The board's control beside the name: a message to someone, which is Home's to start.
    let new_message = icon_button(
        "new-message",
        &icons::COMPOSE,
        "New message",
        |s| s.go_home(HomeView::NewMessage),
        look,
    );
    sidebar_frame(
        s,
        &server.name,
        Some(State::open_server_menu),
        Some(new_message),
        body,
        look,
    )
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
            (
                "No channel yet",
                "Add a channel from the menu under the server’s name, or a server with the rail’s plus.",
            ),
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

/// The server's members, the ones "Find a member" matches when it holds something.
pub fn members(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let mut all: Vec<&MemberSummary> = s
        .session
        .members
        .get(&server.server_id)
        .map(|m| m.iter().collect())
        .unwrap_or_default();
    all.sort_by_cached_key(|m| display_name(m).to_lowercase());
    let total = all.len();
    let wanted = s.member_filter.text.trim().to_lowercase();
    if !wanted.is_empty() {
        all.retain(|m| {
            display_name(m).to_lowercase().contains(&wanted)
                || m.handle.to_lowercase().contains(&wanted)
        });
    }
    let (here, away): (Vec<_>, Vec<_>) = all.iter().copied().partition(|m| {
        s.session
            .presence_of(&server.server_id, &m.account_id)
            .is_some()
    });

    // The board's panel opens with two tabs, Members and Thread. The shell has no threads,
    // so the first tab's words stand alone as the panel's heading.
    let heading = row()
        .w_full()
        // As tall as the board's tab strip in its padding: a segment and 16 either side.
        .h(t::SEGMENT_HEIGHT + t::SPACE_16 * 2.0)
        .px(t::SPACE_18)
        .gap(t::SPACE_6)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .child(
            text("Members")
                .font(t::TEXT_TAB)
                .color(c.ink)
                .role("heading"),
        )
        .child(
            text(total.to_string())
                .font(t::TEXT_SECTION)
                .color(c.ink_muted)
                .label(format!("{total} in this server")),
        );
    let find = row()
        .key("member-search")
        .w_full()
        .h(t::CONTROL_SMALL)
        .px(t::SPACE_12)
        .gap(t::SPACE_8)
        .items_center()
        .radius(t::RADIUS_10)
        .fill(c.field)
        .border(1.0, c.border_composer)
        .focus_within_ring(1.0, -1.0, c.border_active)
        .child(icon(&icons::SEARCH, t::SPACE_16).color(c.ink_quiet))
        .child(
            text_input(&s.member_filter, t::TEXT_MENU)
                .key("find-member")
                .label("Find a member")
                .placeholder("Find a member", c.ink_muted)
                .color(c.ink_field)
                .caret(c.accent, c.accent_soft)
                .grow(1.0)
                .min_w(0.0)
                .on_edit(|s: &mut State| &mut s.member_filter),
        );
    let mut list = col()
        .key("members")
        .w_full()
        .px(t::SPACE_8)
        .pt(t::SPACE_4)
        .pb(t::SPACE_14)
        .role("list")
        .label("Members");
    if !here.is_empty() {
        list = list.child(section_label(
            format!("Online · {}", here.len()),
            t::SPACE_8,
            look,
        ));
        for member in &here {
            list = list.child(member_row(s, server, member, look));
        }
    }
    if !away.is_empty() {
        // With nobody known to be here the instance has said nothing about presence, and
        // a group named for it would only repeat the heading.
        if here.is_empty() {
            list = list.child(el().h(t::SPACE_4));
        } else {
            list = list.child(section_label(
                format!("Offline · {}", away.len()),
                t::SPACE_14,
                look,
            ));
        }
        for member in &away {
            list = list.child(member_row(s, server, member, look));
        }
    }
    if all.is_empty() && !wanted.is_empty() {
        list = list.child(
            text("No member matches that.")
                .font(t::TEXT_META)
                .color(c.ink_quiet)
                .wrap()
                .padding(t::SPACE_10),
        );
    }
    panel("aside", look)
        .w(ASIDE_WIDTH)
        .child(heading)
        .child(
            el().w_full()
                .px(t::SPACE_14)
                .pt(t::SPACE_12)
                .pb(t::SPACE_6)
                .child(find),
        )
        .child(
            col()
                .key("members")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(list)
                .with_scroll_bar(look),
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
    let mut picture = el().child(avatar_of(s, &name, &member.account_id, t::AVATAR_MD, look));
    if let Some(dot) = dot {
        picture = picture.child(status_dot(t::AVATAR_MD, dot, c.surface));
    }
    // The name, quieter for someone who is not here, with "You" beside this account's.
    let mut named = row().w_full().gap(t::SPACE_6).items_center().child(
        text(name.clone())
            .font(t::TEXT_MEMBER)
            .color(if presence.is_some() {
                c.ink
            } else {
                c.ink_body
            })
            .truncate()
            .shrink()
            .min_w(0.0),
    );
    if member.account_id == s.session.me() {
        named = named.child(tag("You", look));
    }
    let mut words = col().grow(1.0).min_w(0.0).child(named);
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
    let key = format!("member-{}", member.account_id);
    let (id, anchor) = (member.account_id.clone(), key.clone());
    row()
        .key(key)
        .w_full()
        .px(t::SPACE_10)
        .py(t::SPACE_6)
        .mb(t::SPACE_2)
        .gap(t::SPACE_12)
        .items_center()
        .radius(t::RADIUS_10)
        .hover(c.hover_member)
        .role("listitem")
        .label(format!("{name}, {status}"))
        // The row opens the person's card, as the board's does.
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(move |s: &mut State| s.open_person(&id, &anchor))
        .child(picture)
        .child(words)
}
