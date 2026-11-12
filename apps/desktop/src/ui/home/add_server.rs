//! Adding a server: a server of your own, or joining one from an invite link or code, with
//! what the invite leads to and on which instance shown before the join (architecture 6.1).
//! One page with two cards, as `AddServerView` in the SwiftUI app's `HomeViews.swift`.
//!
//! Not here yet: an invite to another instance. The code is looked up on the instance this
//! account is on, whatever host the link names.

use lumia_client::app::InvitePreviewSummary;
use rune::{El, ScrollSpec, col, el, text};

use crate::app::State;
use crate::tokens::{self as t};
use crate::ui::components::{Look, card, error_text, field, panel, primary_button};
use crate::ui::conversation;

/// The widest the page's column of cards gets.
const PAGE_WIDTH: f32 = 520.0;

pub fn view(s: &State, look: Look) -> El {
    let c = look.c;
    let header = conversation::header("Add a server", None, look)
        .child(
            el().w(1.0)
                .h(t::SPACE_18 + t::SPACE_2)
                .mx(t::SPACE_6)
                .fill(c.line_strong),
        )
        .child(
            text("Your own, or one you were invited to")
                .font(t::TEXT_BODY)
                .color(c.ink_secondary)
                .truncate(),
        );
    let page = col()
        .w_full()
        .max_w(PAGE_WIDTH)
        .padding(t::SPACE_16)
        .gap(t::SPACE_16)
        .child(create(s, look))
        .child(join(s, look));
    panel("panel", look).w_full().child(header).child(
        col()
            .key("add-server")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(page),
    )
}

/// A server of your own, by name.
fn create(s: &State, look: Look) -> El {
    let form = &s.server_forms.add;
    let mut create = card(
        "Create a server",
        "You own it; invite people once it exists.",
        look,
    )
    .key("create-server-card")
    .w_full()
    .child(field(
        "server-name",
        "Name",
        &form.name,
        "Our place",
        |s| &mut s.server_forms.add.name,
        |s, _| s.create_server(),
        look,
    ));
    if !form.create_error.is_empty() {
        create = create.child(error_text(&form.create_error, look));
    }
    create.child(primary_button(
        "create-server",
        "Create",
        !form.creating && !form.name.text.trim().is_empty(),
        State::create_server,
        look,
    ))
}

/// A server someone invited you to: the link or code, what it leads to, then the join.
fn join(s: &State, look: Look) -> El {
    let form = &s.server_forms.add;
    let mut join = card(
        "Join with an invite",
        "Paste the link someone sent you; what it leads to shows first.",
        look,
    )
    .key("join-server-card")
    .w_full()
    .child(field(
        "invite-link",
        "Invite link or code",
        &form.invite,
        "https://chat.example.com/invite/…",
        |s| &mut s.server_forms.add.invite,
        |s, _| s.look_up_invite(),
        look,
    ));
    if !form.error.is_empty() {
        join = join.child(error_text(&form.error, look));
    }
    join = join.child(primary_button(
        "look-up-invite",
        "Look it up",
        !form.busy && !form.invite.text.trim().is_empty(),
        State::look_up_invite,
        look,
    ));
    if let Some(preview) = &form.preview {
        join = join
            .child(
                text(leads_to(preview))
                    .key("invite-preview")
                    .font(t::TEXT_META)
                    .color(look.c.ink_secondary)
                    .wrap()
                    .w_full(),
            )
            .child(primary_button(
                "join-server",
                &format!("Join {}", preview.server_name),
                !form.busy,
                State::join_invite,
                look,
            ));
    }
    join
}

/// What an invite leads to and what joining shares, in a sentence.
fn leads_to(preview: &InvitePreviewSummary) -> String {
    format!(
        "{} · {} {} on {}, run by {}{}. Your account on this instance joins it; nothing else is shared.",
        preview.server_name,
        preview.member_count,
        if preview.member_count == 1 {
            "member"
        } else {
            "members"
        },
        preview.host,
        if preview.operator_name.is_empty() {
            "an unnamed operator"
        } else {
            &preview.operator_name
        },
        if preview.as_guest {
            " · you would join as a guest"
        } else {
            ""
        },
    )
}
