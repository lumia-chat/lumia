//! The popovers of a server: "New channel" and "Invite people" from the menu under its
//! name, the Channels heading's menu (create a channel, browse them), "Add people" to the
//! channel on screen and "Message someone" in the server (architecture 6.8). Each is
//! returned from `ui::overlay`, so it mounts at the window's root beside what opened it.

use chrono::{DateTime, Local};
use lumia_client::app::{InviteSummary, MemberSummary, ServerSummary};
use rune::{Align, Cursor, El, ScrollSpec, Side, col, el, icon, row, text, text_input};

use super::components::{
    Look, avatar_of, check_row, error_text, field, link_button, menu, menu_glyph, menu_item,
    popover, primary_button, segmented, small_button, tag,
};
use super::menus::SERVER_TITLE;
use crate::app::State;
use crate::session::display_name;
use crate::tokens::{self as t, icons};
use crate::ui::scroll::WithScrollBar;

const CHANNEL_WIDTH: f32 = 320.0;
const INVITE_WIDTH: f32 = 380.0;

/// The keys of the controls the sidebar's and the aside's popovers hang from.
pub const CHANNELS_PLUS: &str = "channels-plus";
pub const DMS_PLUS: &str = "dms-plus";
pub const ADD_PEOPLE: &str = "add-people";

fn current(s: &State) -> Option<&ServerSummary> {
    let id = s.server_id()?;
    s.session.servers().iter().find(|x| x.server_id == id)
}

/// A text channel in the server on screen, by name, hung from `anchor`; it opens once made.
pub fn new_channel(s: &State, anchor: &str, look: Look) -> Option<El> {
    let server = current(s)?;
    let form = &s.server_forms.channel;
    let private = form.private;
    let mut surface = popover(
        "new-channel-popover",
        anchor,
        "New channel",
        &if private {
            "A text channel only the people in it can see.".to_string()
        } else {
            format!("A text channel everyone in {} can see.", server.name)
        },
        CHANNEL_WIDTH,
        look,
    )
    .child(field(
        "channel-name",
        "Name",
        &form.name,
        "general",
        |s| &mut s.server_forms.channel.name,
        |s, _| s.create_channel(),
        look,
    ))
    .child(check_row(
        "channel-private",
        "Private",
        Some("Only people added can see it."),
        private,
        move |s: &mut State| s.set_channel_private(!private),
        look,
    ));
    if !form.error.is_empty() {
        surface = surface.child(error_text(&form.error, look));
    }
    Some(surface.child(primary_button(
        "create-channel",
        "Create",
        !form.busy && !form.name.text.trim().is_empty(),
        State::create_channel,
        look,
    )))
}

/// The Channels heading's plus: a channel to create, for whoever may, and Browse.
pub fn channels_menu(s: &State, look: Look) -> Option<El> {
    let server = current(s)?;
    let mut items = menu(
        "channels-menu",
        CHANNELS_PLUS,
        "Channels",
        Side::Bottom,
        Align::Start,
        look,
    );
    if server.can_manage_channels {
        items = items.child(menu_item(
            "menu-create-channel",
            Some(menu_glyph(&icons::PLUS, look)),
            "Create a channel",
            None,
            State::open_new_channel_here,
            look,
        ));
    }
    Some(items.child(menu_item(
        "menu-browse-channels",
        Some(menu_glyph(&icons::SEARCH, look)),
        "Browse channels",
        None,
        State::open_browse,
        look,
    )))
}

/// The server's members whose name or handle has `filter` in it, by name, without this
/// account.
fn others<'a>(s: &'a State, server_id: &str, filter: &str) -> Vec<&'a MemberSummary> {
    let wanted = filter.trim().to_lowercase();
    let mut all: Vec<&MemberSummary> = s
        .session
        .members
        .get(server_id)
        .map(|m| m.iter().collect())
        .unwrap_or_default();
    all.retain(|m| {
        m.account_id != s.session.me()
            && (wanted.is_empty()
                || display_name(m).to_lowercase().contains(&wanted)
                || m.handle.to_lowercase().contains(&wanted))
    });
    all.sort_by_cached_key(|m| display_name(m).to_lowercase());
    all
}

/// A search field over a picker's list.
fn find(
    key: &'static str,
    edit: &rune::TextEdit,
    focus: impl Fn(&mut State) -> &mut rune::TextEdit + 'static,
    look: Look,
) -> El {
    let c = look.c;
    row()
        .key(format!("{key}-box"))
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
            text_input(edit, t::TEXT_MENU)
                .key(key)
                .label("Find a member")
                .placeholder("Find a member", c.ink_muted)
                .color(c.ink_field)
                .caret(c.accent, c.accent_soft)
                .grow(1.0)
                .min_w(0.0)
                .on_edit(focus),
        )
}

/// A picker's list, scrolling when the window is short for it.
fn picker_list(key: &'static str, label: &str, rows: Vec<El>, look: Look) -> El {
    let mut list = col()
        .key(key)
        .w_full()
        .gap(t::SPACE_2)
        .role("list")
        .label(label);
    for row in rows {
        list = list.child(row);
    }
    col()
        .key(format!("{key}-scroll"))
        .w_full()
        .max_h(t::SPACE_24 * 12.0)
        .shrink()
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(list)
        .with_scroll_bar(key, look)
}

/// What choosing a row of a picker does; none for one that cannot be chosen.
type Choose = Option<Box<dyn Fn(&mut State)>>;

/// One person in a picker: their avatar and name, and `note` at the end when there is one.
fn person_row(
    s: &State,
    key: String,
    member: &MemberSummary,
    note: Option<&str>,
    selected: Option<bool>,
    press: Choose,
    look: Look,
) -> El {
    let c = look.c;
    let name = display_name(member);
    let mut item = row()
        .key(key)
        .w_full()
        .px(t::SPACE_8)
        .py(t::SPACE_6)
        .gap(t::SPACE_10)
        .items_center()
        .radius(t::RADIUS_8)
        .role("listitem")
        .label(match note {
            Some(note) => format!("{name}, {}", note.to_lowercase()),
            None => name.clone(),
        });
    if let Some(selected) = selected {
        let mut tick = el()
            .size(t::SPACE_18, t::SPACE_18)
            .radius(t::RADIUS_5)
            .center()
            .a11y_hidden();
        tick = if selected {
            tick.fill(c.accent)
                .child(icon(&icons::CHECK, t::SPACE_14).color(c.on_accent))
        } else {
            tick.fill(c.field).border(1.0, c.border_form)
        };
        item = item.role("checkbox").checked(selected).child(tick);
    }
    item = item
        .child(avatar_of(s, &name, &member.account_id, t::AVATAR_SM, look))
        .child(
            text(name)
                .font(t::TEXT_MEMBER)
                .color(if press.is_some() { c.ink } else { c.ink_muted })
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        );
    if let Some(note) = note {
        item = item.child(tag(note, look));
    }
    match press {
        Some(press) => item
            .hover(c.hover_member)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| press(s)),
        None => item.disabled(true),
    }
}

/// "Add people" to the channel on screen: the server's members not in it, ticked and added
/// together; one who takes adds as invitations is marked so (architecture 6.8).
pub fn add_people(s: &State, look: Look) -> Option<El> {
    let server = current(s)?;
    let form = &s.channel_forms.add;
    let channel = s.session.channel_summary(&form.channel_id)?;
    let inside = s
        .session
        .channel_members
        .get(&form.channel_id)
        .cloned()
        .unwrap_or_default();
    let rows: Vec<El> = others(s, &server.server_id, &form.filter.text)
        .into_iter()
        .filter(|m| !inside.contains(&m.account_id))
        .map(|m| {
            let id = m.account_id.clone();
            let chosen = form.chosen.contains(&id);
            person_row(
                s,
                format!("add-{id}"),
                m,
                m.channel_adds_by_invitation.then_some("Will be invited"),
                Some(chosen),
                Some(Box::new(move |s: &mut State| s.toggle_chosen(&id))),
                look,
            )
        })
        .collect();
    let empty = rows.is_empty();
    let mut surface = popover(
        "add-people-popover",
        ADD_PEOPLE,
        "Add people",
        &format!("To #{}.", channel.name),
        INVITE_WIDTH,
        look,
    )
    .child(find(
        "add-people-find",
        &form.filter,
        |s| &mut s.channel_forms.add.filter,
        look,
    ));
    surface = if empty {
        surface.child(
            text("Everyone here is in this channel already.")
                .font(t::TEXT_META)
                .color(look.c.ink_quiet)
                .wrap()
                .w_full(),
        )
    } else {
        surface.child(picker_list("add-people-list", "People to add", rows, look))
    };
    if !form.error.is_empty() {
        surface = surface.child(error_text(&form.error, look));
    }
    let count = form.chosen.len();
    Some(surface.child(primary_button(
        "add-chosen",
        &match count {
            0 | 1 => "Add".to_string(),
            n => format!("Add {n} people"),
        },
        !form.busy && count > 0,
        State::add_chosen,
        look,
    )))
}

/// "Message someone" in the server on screen: its members, one to open a server DM with;
/// one who turned server DMs off cannot be chosen.
pub fn new_dm(s: &State, look: Look) -> Option<El> {
    let server = current(s)?;
    let form = &s.channel_forms.new_dm;
    let rows: Vec<El> = others(s, &server.server_id, &form.filter.text)
        .into_iter()
        .map(|m| {
            let id = m.account_id.clone();
            let press: Choose = if m.server_dms_off {
                None
            } else {
                Some(Box::new(move |s: &mut State| s.message_in_server(&id)))
            };
            person_row(
                s,
                format!("dm-{}", m.account_id),
                m,
                m.server_dms_off.then_some("Unavailable"),
                None,
                press,
                look,
            )
        })
        .collect();
    let mut surface = popover(
        "new-dm-popover",
        DMS_PLUS,
        "Message someone",
        &format!("A conversation {} keeps.", server.name),
        CHANNEL_WIDTH,
        look,
    )
    .child(find(
        "new-dm-find",
        &form.filter,
        |s| &mut s.channel_forms.new_dm.filter,
        look,
    ))
    .child(picker_list("new-dm-list", "Members", rows, look));
    if !form.error.is_empty() {
        surface = surface.child(error_text(&form.error, look));
    }
    Some(surface)
}

/// Invites to the server on screen: a new link with an expiry and a use limit, to copy; a
/// one-use invite mailed to an address; and the invites that exist, each revocable.
pub fn invite(s: &State, look: Look) -> Option<El> {
    let server = current(s)?;
    let c = look.c;
    let form = &s.server_forms.invite;
    let mut body = col()
        .w_full()
        .padding(t::SPACE_4)
        .gap(t::SPACE_14)
        .child(choice(
            "invite-expiry",
            "Expires",
            &[(0, "Never"), (24, "In a day"), (168, "In a week")],
            form.expires_in_hours,
            State::set_invite_expiry,
            look,
        ))
        .child(choice(
            "invite-uses",
            "Uses",
            &[(0, "Unlimited"), (1, "Once"), (10, "Ten")],
            form.max_uses,
            State::set_invite_uses,
            look,
        ));
    if !form.error.is_empty() {
        body = body.child(error_text(&form.error, look));
    }
    body = body.child(primary_button(
        "make-invite",
        "Make a link",
        !form.busy,
        State::make_invite,
        look,
    ));
    // The link just made sits under the button that made it, where it is seen without
    // scrolling: the popover is no taller than the window.
    if let Some(made) = &form.made {
        body = body
            .child(
                row()
                    .key("invite-link-made")
                    .w_full()
                    .px(t::SPACE_12)
                    .py(t::SPACE_10)
                    .radius(t::RADIUS_12)
                    .fill(c.inset)
                    .child(
                        text(made.url.clone())
                            .font(t::TEXT_META)
                            .color(c.ink)
                            .selectable(true)
                            .wrap()
                            .grow(1.0)
                            .min_w(0.0)
                            .label("Invite link"),
                    ),
            )
            .child(row().w_full().child(small_button(
                "copy-invite",
                if form.copied { "Copied" } else { "Copy link" },
                true,
                State::copy_invite,
                look,
            )));
    }
    body = body
        // The invite as a mail (architecture 6.2): one use, a week, to the address.
        .child(field(
            "invite-email",
            "Email",
            &form.email,
            "pat@example.com",
            |s| &mut s.server_forms.invite.email,
            |s, _| s.email_invite(),
            look,
        ))
        .child(row().w_full().child(small_button(
            "email-invite",
            "Send by email",
            !form.busy && !form.email.text.trim().is_empty(),
            |s, _| s.email_invite(),
            look,
        )));
    if !form.emailed.is_empty() {
        body = body.child(
            text(format!(
                "Sent to {}; it works once, within a week.",
                form.emailed
            ))
            .font(t::TEXT_META)
            .color(c.ink_secondary)
            .wrap()
            .w_full(),
        );
    }
    if !form.list.is_empty() {
        let mut list = col()
            .key("invites")
            .w_full()
            .gap(t::SPACE_8)
            .role("list")
            .label("Invites");
        for invite in &form.list {
            list = list.child(invite_row(invite, look));
        }
        body = body
            .child(text("Invites").font(t::TEXT_SECTION).color(c.ink_muted))
            .child(list);
    }
    // The body scrolls when the window is too short for it. Its own padding, taken back by
    // the margin, keeps the focus rings of what it holds inside the scrolled area.
    Some(
        popover(
            "invite-popover",
            SERVER_TITLE,
            "Invite people",
            &format!("A link to join {}.", server.name),
            INVITE_WIDTH,
            look,
        )
        .child(
            col()
                .key("invite-body")
                .mx(-t::SPACE_4)
                .my(-t::SPACE_4)
                .shrink()
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body)
                .with_scroll_bar("invite-body", look),
        ),
    )
}

/// One of the link's limits: a label over the choices.
fn choice(
    key: &str,
    label: &str,
    options: &[(u32, &str)],
    selected: u32,
    pick: fn(&mut State, u32),
    look: Look,
) -> El {
    col()
        .w_full()
        .gap(t::SPACE_6)
        .child(
            text(label)
                .font(t::TEXT_LABEL)
                .color(look.c.ink_body)
                .a11y_hidden(),
        )
        .child(segmented(key, label, options, selected, pick, look))
}

fn invite_row(invite: &InviteSummary, look: Look) -> El {
    let c = look.c;
    let described = describe_invite(invite);
    let code = invite.code.clone();
    row()
        .key(format!("invite-{}", invite.code))
        .w_full()
        .gap(t::SPACE_10)
        .items_center()
        .role("listitem")
        .label(format!("{}, {described}", invite.code))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text(invite.code.clone())
                        .font(t::TEXT_MEMBER)
                        .color(c.ink)
                        .truncate(),
                )
                .child(
                    text(described)
                        .font(t::TEXT_META)
                        .color(c.ink_quiet)
                        .truncate(),
                ),
        )
        .child(
            link_button(
                format!("revoke-{}", invite.code),
                "Revoke",
                move |s| s.revoke_invite(&code),
                look,
            )
            .label(format!("Revoke invite {}", invite.code)),
        )
}

/// An invite's uses and expiry in a line: who it was mailed to, how often it was used of
/// how often it may be, and when it stops working.
pub fn describe_invite(invite: &InviteSummary) -> String {
    let to = if invite.email.is_empty() {
        String::new()
    } else {
        format!("to {} · ", invite.email)
    };
    let uses = if invite.max_uses == 0 {
        format!("{} uses", invite.uses)
    } else {
        format!("{}/{} uses", invite.uses, invite.max_uses)
    };
    let expiry = invite
        .expires_at_micros
        .and_then(DateTime::from_timestamp_micros)
        .map_or_else(
            || "no expiry".to_string(),
            |at| {
                format!(
                    "expires {}",
                    at.with_timezone(&Local).format("%b %-d, %-I:%M %p")
                )
            },
        );
    format!("{to}{uses} · {expiry}")
}
