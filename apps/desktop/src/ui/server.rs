//! A server on screen: the channels this account is in and its server DMs in the sidebar,
//! each section with its plus (architecture 6.8), the open channel or DM as the
//! conversation, and the open channel's members as the aside (`channels`), grouped by
//! presence; the server's members where no channel is open. The name opens the server's
//! menu (`menus::server`).

use lumia_client::app::{ChannelSummary, MemberSummary, ServerSummary};
use rune::{Cursor, El, ScrollSpec, col, el, icon, row, text, text_input};

use super::components::{
    Look, avatar_of, count_badge, error_text, icon_button, link_button, notice, panel,
    section_label, small_button, status_dot, tag,
};
use super::conversation::{self, Chrome};
use super::server_popovers as popovers;
use super::shell::{ASIDE_WIDTH, sidebar_frame};
use crate::app::channels::ServerPage;
use crate::app::{HomeView, State};
use crate::session::display_name;
use crate::tokens::{self as t, icons};
use crate::ui::scroll::WithScrollBar;

/// The server's name; the invitations waiting, its channels this account is in (public, then
/// private under a heading of their own, as the boards draw them) and its server DMs
/// (architecture 6.8 and 6.4), each section with its plus where it has one.
pub fn sidebar(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let open = s.session.channel();
    let channel_list = |key: &str, label: &str| {
        col()
            .key(key)
            .w_full()
            .gap(t::SPACE_2)
            .role("list")
            .label(label)
    };
    let (mut list, mut private) = (
        channel_list("channel-list", "Channels"),
        channel_list("private-channels", "Private channels"),
    );
    let (mut any, mut any_private) = (false, false);
    for channel in s.session.channels_of(&server.server_id) {
        let id = channel.channel_id.clone();
        let glyph = if channel.private { "" } else { "#" };
        let unread = s.session.unread(&channel.channel_id);
        let row = nav_row(
            format!("channel-{}", channel.channel_id),
            &channel.name,
            &with_unread(&format!("{} channel", channel.name), unread),
            NavLead::Glyph(glyph, channel.private),
            open == Some(channel.channel_id.as_str()),
            unread,
            move |s: &mut State| s.select_channel(&id),
            look,
        );
        if channel.private {
            any_private = true;
            private = private.child(row);
        } else {
            any = true;
            list = list.child(row);
        }
    }
    let mut channels = col()
        .w_full()
        .child(section_head(
            "Channels",
            popovers::CHANNELS_PLUS,
            "Add channels",
            State::open_channels_menu,
            look,
        ))
        .child(invitations(s, server, look))
        .child(list);
    if !any && !any_private {
        channels = channels.child(
            text("You are in no channels here. Browse them with the plus.")
                .font(t::TEXT_META)
                .color(c.ink_quiet)
                .wrap()
                .padding(t::SPACE_10),
        );
    }
    // The boards' Private: the private channels this account is in, under a heading with no
    // plus (Create a channel makes one private), drawn only when there are any.
    let private = any_private.then(|| {
        col()
            .w_full()
            .child(section_label("Private", t::SPACE_4, look))
            .child(private)
    });
    let mut dms = col()
        .key("server-dms")
        .w_full()
        .gap(t::SPACE_2)
        .role("list")
        .label("Direct messages");
    for dm in s.session.server_dms_of(&server.server_id) {
        let partner = s.session.dm_partner(dm).to_string();
        let name = s.session.name_of(&partner);
        let id = dm.channel_id.clone();
        let unread = s.session.unread(&dm.channel_id);
        dms = dms.child(nav_row(
            format!("dm-{}", dm.channel_id),
            &name,
            &with_unread(&format!("Conversation with {name}"), unread),
            NavLead::Avatar(partner),
            open == Some(dm.channel_id.as_str()),
            unread,
            move |s: &mut State| s.select_channel(&id),
            look,
        ));
    }
    let messages = if server.can_start_dms {
        section_head(
            "Direct messages",
            popovers::DMS_PLUS,
            "Message someone",
            State::open_new_dm,
            look,
        )
    } else {
        section_label("Direct messages", t::SPACE_14, look)
    };
    // The boards' quick tiles under the server's name: Threads, Mentions and Saved, of which
    // Mentions is built.
    let (_, mentions) = s.session.server_unread(&server.server_id);
    let tiles = row()
        .key("tiles")
        .w_full()
        .gap(t::SPACE_6)
        .pb(t::SPACE_12)
        .child(
            super::home::quick_tile(
                "Mentions",
                &with_unread("Mentions", (mentions, mentions)),
                &icons::MENTION,
                mentions as usize,
                s.server_page() == Some(ServerPage::Mentions),
                State::open_server_mentions,
                look,
            )
            .key("server-mentions"),
        );
    // The boards' sections stand 12 apart: Channels, Private, Direct messages.
    let body = col()
        .key("channels")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(t::SPACE_12)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(channels)
        .children(private)
        .child(col().w_full().child(messages).child(dms))
        .with_scroll_bar("channels", look);
    // The board's control beside the name: a message to someone, which is Home's to start.
    let new_message = icon_button(
        "new-message",
        &icons::COMPOSE,
        "New message",
        |s| s.go_home(HomeView::NewMessage),
        look,
    );
    let body = col().w_full().grow(1.0).min_h(0.0).child(tiles).child(body);
    sidebar_frame(
        s,
        &server.name,
        Some(State::open_server_menu),
        Some(new_message),
        body,
        look,
    )
}

/// What a sidebar row leads with: a channel's hash (or a lock, for a private one), the
/// other person's avatar in a server DM, or a glyph (Mentions).
pub(super) enum NavLead {
    Glyph(&'static str, bool),
    Avatar(String),
    Icon(&'static rune::Icon),
}

/// A row's name for assistive technology with what is unread in it.
pub(super) fn with_unread(label: &str, (unread, mentions): (u32, u32)) -> String {
    match (unread, mentions) {
        (_, 0) if unread > 0 => format!("{label}, unread"),
        (0, 0) => label.to_string(),
        (_, 1) => format!("{label}, 1 mention"),
        (_, n) => format!("{label}, {n} mentions"),
    }
}

/// A row of the sidebar: a channel, a server DM, Mentions. 700 on the open one; 600 in the
/// strong ink on one with unread messages, and their count for this account at its end.
#[allow(clippy::too_many_arguments)]
pub(super) fn nav_row(
    key: String,
    name: &str,
    label: &str,
    lead: NavLead,
    active: bool,
    (unread, mentions): (u32, u32),
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let bold = !active && unread > 0;
    let style = if active {
        t::TEXT_NAV_ACTIVE
    } else if bold {
        t::TEXT_NAV_UNREAD
    } else {
        t::TEXT_NAV
    };
    let lead_ink = if active {
        c.accent
    } else if bold {
        c.ink_muted
    } else {
        c.ink_faint
    };
    // The hash is a character of the row's type in a column of its own, as the board sets
    // it; a lock stands in the same column.
    let lead = match lead {
        NavLead::Glyph(_, true) => el()
            .w(t::SPACE_10)
            .center()
            .a11y_hidden()
            .child(icon(&icons::LOCK, t::SPACE_12).color(lead_ink)),
        NavLead::Glyph(glyph, false) => text(glyph)
            .font(style)
            .color(lead_ink)
            .w(t::SPACE_10)
            .a11y_hidden(),
        NavLead::Avatar(id) => {
            el().a11y_hidden()
                .child(super::components::avatar(name, &id, t::AVATAR_XS, look))
        }
        NavLead::Icon(glyph) => el()
            .w(t::SPACE_10)
            .center()
            .a11y_hidden()
            .child(icon(glyph, t::SPACE_14).color(lead_ink)),
    };
    let mut item = row()
        .key(key)
        .w_full()
        .h(t::ROW_HEIGHT)
        .px(t::SPACE_10)
        .gap(t::SPACE_8)
        .items_center()
        .radius(t::RADIUS_8)
        .role("button")
        .label(label)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(press)
        .child(lead)
        .child(
            text(name.to_string())
                .font(style)
                .color(if active {
                    c.ink_selected
                } else if bold {
                    c.ink
                } else {
                    c.ink_quiet
                })
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        );
    if mentions > 0 {
        item = item.child(count_badge(mentions, false, look));
    }
    if active {
        item = item.fill(c.raised);
        for layer in look.shadows.selected {
            item = item.box_shadow(*layer);
        }
    } else {
        item = item.hover(c.hover_row);
    }
    item
}

/// A section's heading with its plus at the end, which opens a popover hung from it.
fn section_head(
    label: &str,
    key: &'static str,
    action: &str,
    press: fn(&mut State),
    look: Look,
) -> El {
    row()
        .w_full()
        .items_center()
        .pr(t::SPACE_4)
        .child(section_label(label, t::SPACE_4, look).grow(1.0))
        .child(icon_button(key, &icons::PLUS, action, press, look))
}

/// The invitations into the server's channels this account has not answered: each with the
/// way in and the way to say no.
fn invitations(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let mut all = col()
        .key("invitations")
        .w_full()
        .gap(t::SPACE_6)
        .role("list")
        .label("Invitations");
    let mut any = false;
    for invitation in s.session.invitations_of(&server.server_id) {
        any = true;
        let channel = &invitation.channel;
        let by = s.session.name_of(&invitation.invited_by_account_id);
        let (join, dismiss) = (channel.channel_id.clone(), channel.channel_id.clone());
        let joining = s.channel_forms.joining.as_deref() == Some(channel.channel_id.as_str());
        all = all.child(
            col()
                .key(format!("invitation-{}", channel.channel_id))
                .w_full()
                .px(t::SPACE_10)
                .py(t::SPACE_8)
                .gap(t::SPACE_6)
                .radius(t::RADIUS_10)
                .fill(c.inset)
                .role("listitem")
                .label(format!("{by} invited you to {}", channel.name))
                .child(
                    text(format!("#{} · {by} invited you", channel.name))
                        .font(t::TEXT_META)
                        .color(c.ink_body)
                        .wrap()
                        .w_full(),
                )
                .child(
                    row()
                        .gap(t::SPACE_8)
                        .items_center()
                        .child(small_button(
                            format!("join-{}", channel.channel_id),
                            "Join",
                            !joining,
                            move |s, _| s.join_channel(&join),
                            look,
                        ))
                        .child(link_button(
                            format!("dismiss-{}", channel.channel_id),
                            "Dismiss",
                            move |s| s.dismiss_invitation(&dismiss),
                            look,
                        )),
                ),
        );
    }
    if !any {
        return el();
    }
    if !s.channel_forms.invitation_error.is_empty() {
        all = all.child(error_text(&s.channel_forms.invitation_error, look));
    }
    all.pb(t::SPACE_8)
}

/// The open channel or server DM, or what to say when there is none; or a page in its
/// place: the server's settings, Browse, this account's settings in the server.
pub fn main(s: &State, server: &ServerSummary, look: Look) -> El {
    if s.server_settings_open() {
        return super::server_settings::view(s, server, look);
    }
    match s.server_page() {
        Some(ServerPage::Browse) => return super::channels::browse(s, server, look),
        Some(ServerPage::YourSettings) => return super::channels::your_settings(s, server, look),
        Some(ServerPage::Mentions) => {
            return super::mentions::view(s, Some(&server.server_id), look);
        }
        None => {}
    }
    let Some(channel) = s
        .session
        .channel()
        .and_then(|id| s.session.channel_summary(id))
    else {
        return conversation::vacant(
            &server.name,
            &icons::HASH,
            (
                "No channel open",
                "Choose one in the sidebar, or browse the server’s channels with the plus beside Channels.",
            ),
            look,
        );
    };
    if channel.kind == "SERVER_DM" {
        return server_dm(s, server, channel, look);
    }
    let name = channel.name.as_str();
    let mut extras = conversation::Extras {
        topic: Some(channel.topic.clone()),
        empty_glyph: channel.private.then_some(&icons::LOCK),
        ..conversation::Extras::default()
    };
    let joined = s.session.joined(&channel.channel_id);
    if !joined {
        extras.instead_of_composer = Some(super::channels::join_bar(s, channel, look));
    }
    conversation::view_with(
        s,
        Chrome {
            title: name,
            glyph: Some(if channel.private {
                &icons::LOCK
            } else {
                &icons::HASH
            }),
            list_label: format!("Messages in {name}"),
            placeholder: format!("Message #{name}"),
            empty: (
                "Nothing here yet",
                if joined {
                    "Say something."
                } else {
                    "Join to say something."
                },
            ),
            aside: Some("Members"),
        },
        extras,
        look,
    )
}

/// A server DM: titled by the other person, with what it is said once at its top
/// (architecture 6.4).
fn server_dm(s: &State, server: &ServerSummary, channel: &ChannelSummary, look: Look) -> El {
    let partner = s.session.dm_partner(channel);
    let name = s.session.name_of(partner);
    let note = notice(
        &icons::SHIELD,
        format!(
            "Messages here are kept by {} and follow its message history settings. For an encrypted conversation, message privately.",
            server.name
        ),
        look,
    )
    .key("server-dm-note");
    conversation::view_with(
        s,
        Chrome {
            title: &name,
            glyph: None,
            list_label: format!("Messages with {name}"),
            placeholder: format!("Message {name}"),
            empty: ("No messages yet", "Say hello."),
            aside: None,
        },
        conversation::Extras {
            notices: vec![note],
            empty_glyph: Some(&icons::USER),
            ..conversation::Extras::default()
        },
        look,
    )
}

/// What stands beside the conversation: the open channel's members, none beside a server
/// DM, and the server's members where no channel is open.
pub fn aside(s: &State, server: &ServerSummary, look: Look) -> Option<El> {
    // A page in place of the channel has the server's members beside it.
    if s.server_settings_open() || s.server_page().is_some() {
        return Some(members(s, server, look));
    }
    let open = s
        .session
        .channel()
        .and_then(|id| s.session.channel_summary(id));
    match open {
        Some(channel) if channel.kind == "SERVER_DM" => None,
        Some(channel) => Some(super::channels::channel_members(s, server, channel, look)),
        None => Some(members(s, server, look)),
    }
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
    let find = find_member(s, look);
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
                .with_scroll_bar("members", look),
        )
}

pub(super) fn member_row(
    s: &State,
    server: &ServerSummary,
    member: &MemberSummary,
    look: Look,
) -> El {
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

/// "Find a member": what it holds narrows the aside's list.
pub(super) fn find_member(s: &State, look: Look) -> El {
    let c = look.c;
    row()
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
        )
}
