//! The menus and the profile card of the Quiet Split: the menu under a server's name, the
//! account menu on the rail's self avatar, and the card a person's avatar or row opens. Each
//! is returned from `ui::overlay`, so it mounts at the window's root beside its anchor.
//!
//! Where they leave the boards. The server's name has no menu drawn, only its chevron; this
//! one holds what the shell can do in a server: invite, browse its channels, make one (for
//! whoever may), this account's own settings there, and the server's settings for whoever
//! may change them. The account menu has no Keyboard shortcuts, which the shell does not
//! have, and has Devices, which it does. The card has no team, no local time and no call
//! button, none of which the product knows; it shows the pronouns, the custom status and the
//! bio of the person's verified profile, and in a server offers a server DM first and a
//! personal conversation beside it (architecture 6.4).

use lumia_client::app::MemberSummary;
use rune::{Align, Cursor, El, Side, col, el, icon, row, text};

use super::components::{
    CARD_WIDTH, Look, action_button, avatar_of, menu, menu_glyph, menu_item, menu_rule,
    section_label, shadowed, tag,
};
use crate::app::{HomeView, State};
use crate::session::{Connection, display_name};
use crate::tokens::{self as t, icons};

/// The key of the sidebar's title button, which the server's menu and the popovers its
/// rows open hang from.
pub const SERVER_TITLE: &str = "server-title";
/// The key of the rail's self avatar.
pub const SELF_AVATAR: &str = "self";

/// The menu under the server's name: what the shell can do to the server on screen.
pub fn server(s: &State, look: Look) -> Option<El> {
    let id = s.server_id()?;
    let server = s.session.servers().iter().find(|x| x.server_id == id)?;
    let mut items = menu(
        "server-menu",
        SERVER_TITLE,
        &server.name,
        Side::Bottom,
        Align::Start,
        look,
    )
    .child(menu_item(
        "menu-invite",
        Some(menu_glyph(&icons::MEMBER_JOINED, look)),
        "Invite people",
        None,
        State::open_invites,
        look,
    ))
    .child(menu_item(
        "menu-browse",
        Some(menu_glyph(&icons::SEARCH, look)),
        "Browse channels",
        None,
        State::open_browse,
        look,
    ));
    if server.can_manage_channels {
        items = items.child(menu_item(
            "menu-new-channel",
            Some(menu_glyph(&icons::HASH, look)),
            "New channel",
            None,
            State::open_new_channel,
            look,
        ));
    }
    items = items.child(menu_rule(look)).child(menu_item(
        "menu-your-settings",
        Some(menu_glyph(&icons::USER, look)),
        &format!("Your settings in {}", server.name),
        None,
        State::open_your_settings,
        look,
    ));
    // Offered to whoever may change them; the instance decides all the same.
    if server.can_manage {
        items = items.child(menu_item(
            "menu-server-settings",
            Some(menu_glyph(&icons::SETTINGS, look)),
            "Server settings",
            None,
            State::open_server_settings,
            look,
        ));
    }
    Some(items)
}

/// The account menu: who is signed in, the status they show, and the ways to their
/// profile, the preferences, their devices and out.
pub fn account(s: &State, look: Look) -> Option<El> {
    let c = look.c;
    let snapshot = s.session.snapshot.as_ref()?;
    let connected = s.session.connection == Connection::Connected;
    let chosen = s
        .account
        .menu_status
        .as_ref()
        .map_or("ONLINE", |(status, _)| status.as_str());
    let (dot, words) = if connected {
        match chosen {
            "AWAY" => (c.away, "Away".to_string()),
            "DO_NOT_DISTURB" => (c.dnd, "Do not disturb".to_string()),
            _ => (c.online, "Online".to_string()),
        }
    } else {
        (c.ink_muted, s.session.connection.label())
    };
    let handle = if snapshot.handle.is_empty() {
        words
    } else {
        format!("@{} · {words}", snapshot.handle)
    };
    let who = row()
        .w_full()
        .px(t::SPACE_10)
        .pt(t::SPACE_8)
        .pb(t::SPACE_10)
        .gap(t::SPACE_10)
        .items_center()
        .child(
            el().child(avatar_of(
                s,
                s.session.self_name(),
                s.session.me(),
                t::AVATAR_LG,
                look,
            ))
            .child(super::components::status_dot(t::AVATAR_LG, dot, c.surface)),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text(s.session.self_name())
                        .font(t::TEXT_AUTHOR)
                        .color(c.ink)
                        .truncate(),
                )
                .child(
                    text(handle)
                        .font(t::TEXT_META)
                        .color(c.ink_muted)
                        .truncate(),
                ),
        );
    let status = |key: &'static str, value: &'static str, label: &'static str, dot| {
        let lead = el()
            .size(t::SPACE_8, t::SPACE_8)
            .mx(t::SPACE_4)
            .radius(t::SPACE_4)
            .fill(dot);
        let tick = (chosen == value).then(|| icon(&icons::CHECK, t::SPACE_16).color(c.accent));
        menu_item(
            key,
            Some(lead),
            label,
            tick,
            move |s: &mut State| s.choose_status(value),
            look,
        )
        .role("menuitemradio")
        .checked(chosen == value)
    };
    Some(
        menu(
            "account-menu",
            SELF_AVATAR,
            "Your account",
            Side::Right,
            Align::End,
            look,
        )
        .child(who)
        .child(menu_rule(look))
        .child(section_label("Status", t::SPACE_6, look))
        .child(status("status-online", "ONLINE", "Online", c.online))
        .child(status("status-away", "AWAY", "Away", c.away))
        .child(status(
            "status-dnd",
            "DO_NOT_DISTURB",
            "Do not disturb",
            c.dnd,
        ))
        .child(menu_rule(look))
        .child(menu_item(
            "menu-profile",
            Some(menu_glyph(&icons::USER, look)),
            "View profile",
            None,
            |s: &mut State| s.go_home(HomeView::Profile),
            look,
        ))
        .child(menu_item(
            "menu-preferences",
            Some(menu_glyph(&icons::PREFERENCES, look)),
            "Preferences",
            None,
            |s: &mut State| s.go_home(HomeView::Preferences),
            look,
        ))
        .child(menu_item(
            "menu-devices",
            Some(menu_glyph(&icons::MONITOR, look)),
            "Devices",
            None,
            |s: &mut State| s.go_home(HomeView::Devices),
            look,
        ))
        .child(menu_rule(look))
        .child(menu_item(
            "menu-sign-out",
            Some(menu_glyph(&icons::SIGN_OUT, look)),
            "Sign out",
            None,
            State::sign_out,
            look,
        )),
    )
}

/// The profile card of `State::person`: a banner in the person's tone with their avatar
/// over its edge, who they are, whether they are here, what their profile says, and the
/// way to a conversation with them.
pub fn person(s: &State, look: Look) -> Option<El> {
    let c = look.c;
    let open = s.person.as_ref()?;
    let found = s.session.person(&open.account_id);
    let me = open.account_id == s.session.me();
    let name = s.session.name_of(&open.account_id);
    let presence = s
        .server_id()
        .and_then(|server| s.session.presence_of(server, &open.account_id));
    let (ground, _) = t::tone(&open.account_id, c);

    let close = el()
        .key("card-close")
        .pin(None, Some(t::SPACE_8), Some(t::SPACE_8), None)
        .size(t::ROW_HEIGHT, t::ROW_HEIGHT)
        .radius(t::RADIUS_10)
        .center()
        .role("button")
        .label("Close profile")
        .hover(c.hover_icon)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(State::close_popover)
        .child(icon(&icons::CLOSE, t::SPACE_16).color(c.ink_body));
    let banner = el().w_full().h(t::CARD_BANNER).fill(ground).child(close);

    // The avatar sits in a ring of the card's own colour, half over the banner.
    let inner = t::AVATAR_CARD - t::SPACE_8;
    let mut picture = el()
        .size(t::AVATAR_CARD, t::AVATAR_CARD)
        .mt(-t::AVATAR_CARD / 2.0)
        .radius(t::AVATAR_CARD / 2.0)
        .fill(c.surface)
        .center()
        .child(avatar_of(s, &name, &open.account_id, inner, look));
    let dot = presence.map(|(status, _)| match status.as_str() {
        "AWAY" => c.away,
        "DO_NOT_DISTURB" => c.dnd,
        _ => c.online,
    });
    if let Some(dot) = dot {
        let size = t::SPACE_18;
        let at = t::AVATAR_CARD - size + 1.0;
        picture = picture.child(
            el().absolute(at, at)
                .size(size, size)
                .radius(size / 2.0)
                .fill(dot)
                .border(3.0, c.surface),
        );
    }

    let mut named = row().w_full().gap(t::SPACE_8).items_center().child(
        text(name.clone())
            .font(t::TITLE_SERVER)
            .color(c.ink)
            .truncate()
            .shrink()
            .min_w(0.0)
            .role("heading"),
    );
    if me {
        named = named.child(tag("You", look));
    }
    let mut who = col().w_full().gap(t::SPACE_2).child(named);
    if let Some(handle) = found.map(|p| p.handle.as_str()).filter(|h| !h.is_empty()) {
        who = who.child(
            text(format!("@{handle}"))
                .font(t::TEXT_CARD_LINE)
                .color(c.ink_muted)
                .truncate(),
        );
    }

    let (dot, words) = match presence.map(|(status, _)| status.as_str()) {
        Some("AWAY") => (c.away, "Away"),
        Some("DO_NOT_DISTURB") => (c.dnd, "Do not disturb"),
        Some(_) => (c.online, "Online"),
        None => (c.ink_faint, "Offline"),
    };
    let here = row()
        .gap(t::SPACE_8)
        .items_center()
        .child(
            el().size(t::SPACE_8, t::SPACE_8)
                .radius(t::SPACE_4)
                .fill(dot)
                .a11y_hidden(),
        )
        .child(text(words).font(t::TEXT_CARD_LINE).color(c.ink_body));

    let mut card = col()
        .w_full()
        .px(t::SPACE_18)
        .pb(t::SPACE_18)
        .gap(t::SPACE_14)
        .child(picture)
        .child(who);
    // Presence is a server's to say; at Home there is none to show.
    if s.server_id().is_some() {
        card = card.child(here);
    }
    if let Some(details) = details(found, presence.map(|(_, custom)| custom.as_str()), look) {
        card = card.child(details);
    }
    if !me {
        card = card.child(ways_to_talk(s, &name, found, look));
    }

    Some(
        shadowed(
            col()
                .key("person-card")
                .w(CARD_WIDTH)
                .radius(t::RADIUS_18)
                .fill(c.surface)
                .border(1.0, c.line_strong),
            look.shadows.floating,
        )
        .clip_rounded(t::RADIUS_18)
        .anchored_fit(open.anchor.clone(), Side::Left, Align::Start, t::SPACE_8)
        .modal(true, State::close_popover)
        .role("dialog")
        .label(format!("{name}, profile"))
        .child(banner)
        .child(card),
    )
}

/// How to talk to the person on the card. In a server, a server DM first ("Message"),
/// unless they turned those off there, and a personal conversation beside it ("Message
/// privately"); at Home, the personal one alone.
fn ways_to_talk(s: &State, name: &str, found: Option<&MemberSummary>, look: Look) -> El {
    let first = name.split_whitespace().next().unwrap_or(name).to_string();
    let in_server = s.server_id().is_some();
    if !in_server {
        return action_button(
            "card-message",
            &format!("Message {first}"),
            true,
            State::message_person,
            look,
        )
        .w_full();
    }
    let c = look.c;
    let form = &s.channel_forms;
    let mut ways = col().w_full().gap(t::SPACE_8);
    let server = s
        .server_id()
        .and_then(|id| s.session.servers().iter().find(|x| x.server_id == id));
    let takes = !found.is_some_and(|p| p.server_dms_off);
    let may = server.is_some_and(|x| x.can_start_dms);
    if takes && may {
        ways = ways.child(
            action_button(
                "card-message",
                &format!("Message {first}"),
                !form.dm_busy,
                State::message_person_here,
                look,
            )
            .w_full(),
        );
    } else if !takes {
        ways = ways.child(
            text("Doesn't take messages here")
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .w_full()
                .align(Align::Center),
        );
    }
    if !form.dm_error.is_empty() {
        ways = ways.child(super::components::error_text(&form.dm_error, look));
    }
    ways.child(
        super::components::secondary_button(
            "card-message-privately",
            "Message privately",
            true,
            State::message_person,
            look,
        )
        .w_full(),
    )
}

/// The inset block of a card: what the person's profile says about them, a line each.
fn details(person: Option<&MemberSummary>, custom: Option<&str>, look: Look) -> Option<El> {
    let c = look.c;
    let person = person?;
    let shown = display_name(person);
    let lines: Vec<(&'static rune::Icon, &str)> = [
        (&icons::EMOJI, custom.unwrap_or_default()),
        (&icons::USER, person.pronouns.as_str()),
        (&icons::MENTION, person.bio.as_str()),
    ]
    .into_iter()
    .filter(|(_, words)| !words.trim().is_empty() && *words != shown)
    .collect();
    if lines.is_empty() {
        return None;
    }
    let mut block = col()
        .w_full()
        .px(t::SPACE_12)
        .py(t::SPACE_10)
        .gap(t::SPACE_8)
        .radius(t::RADIUS_12)
        .fill(c.inset);
    for (glyph, words) in lines {
        block = block.child(
            row()
                .w_full()
                .gap(t::SPACE_10)
                .items_start()
                .child(
                    el().h(t::CARD_LINE)
                        .center()
                        .child(icon(glyph, t::SPACE_16).color(c.ink_muted)),
                )
                .child(
                    text(words.to_string())
                        .font(t::TEXT_CARD_LINE)
                        .color(c.ink_body)
                        .wrap()
                        .grow(1.0)
                        .min_w(0.0),
                ),
        );
    }
    Some(block)
}
