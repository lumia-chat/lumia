//! The popovers a server's sidebar opens: "New channel" and "Invite people", as
//! `channelPopover` and `invitePopover` in the SwiftUI app's `ShellView.swift`. Each is
//! returned from `ui::overlay`, so it mounts at the window's root beside its link.

use chrono::{DateTime, Local};
use lumia_client::app::{InviteSummary, ServerSummary};
use rune::{El, ScrollSpec, col, row, text};

use super::components::{
    Look, error_text, field, link_button, popover, primary_button, segmented, small_button,
};
use crate::app::State;
use crate::tokens::{self as t};

const CHANNEL_WIDTH: f32 = 320.0;
const INVITE_WIDTH: f32 = 380.0;

fn current(s: &State) -> Option<&ServerSummary> {
    let id = s.server_id()?;
    s.session.servers().iter().find(|x| x.server_id == id)
}

/// A text channel in the server on screen, by name; it opens once made.
pub fn new_channel(s: &State, look: Look) -> Option<El> {
    let server = current(s)?;
    let form = &s.server_forms.channel;
    let mut surface = popover(
        "new-channel-popover",
        "new-channel",
        "New channel",
        &format!("A text channel everyone in {} can see.", server.name),
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
            "invite-people",
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
                .child(body),
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
