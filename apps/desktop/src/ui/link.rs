//! Link this computer as a device of an identity (identity spec 4.2): show the request for
//! a device that holds the identity to approve, then claim the code it gives and sign in by
//! identity proof. The same card and wording as the iOS app's `LinkView.swift` and the web's `link.tsx`.
//!
//! Reached from sign-in when the instance allows identity-proof sign-in, and from Devices
//! and Profile when the account is signed in with no identity on this computer.
//!
//! Not here: the request as a QR code (the other shells show its text too), and approving
//! another device's request from this one, which only the CLI does today.

use rune::{El, ScrollSpec, Tx, col, el, row, text};

use super::components::{Look, card, error_text, field, primary_button, small_button};
use super::sign_in::foot_link;
use crate::app::State;
use crate::tokens::{self as t};
use crate::ui::scroll::WithScrollBar;

pub fn view(s: &State, look: Look) -> El {
    let c = look.c;
    let form = &s.account.link;
    let note = |words: &str| {
        text(words)
            .font(t::TEXT_BODY)
            .color(c.ink_secondary)
            .wrap()
            .w_full()
    };
    let mut card = card(
        "Link this computer",
        "A device you already use approves this one.",
        look,
    )
    .key("link-card")
    .no_window_drag();
    if form.request.is_empty() {
        card = card
            .child(note("This computer gets its own device key."))
            .child(primary_button(
                "start-link",
                "Start linking",
                !form.starting,
                State::start_link,
                look,
            ));
    } else {
        card = card
            .child(note(
                "On a device that holds your identity, approve this request and enter the code it gives you.",
            ))
            .child(request(&form.request, look))
            .child(row().w_full().child(small_button(
                "copy-request",
                if form.copied { "Copied" } else { "Copy request" },
                true,
                State::copy_link_request,
                look,
            )))
            .child(field(
                "link-code",
                "Code from the approving device",
                &form.code,
                "8 characters",
                |s| &mut s.account.link.code,
                claim,
                look,
            ));
    }
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    if !form.request.is_empty() {
        card = card.child(primary_button(
            "claim-link",
            "Claim and sign in",
            !form.busy && !form.code.text.trim().is_empty(),
            State::claim_link,
            look,
        ));
    }
    // Back to where the screen was opened from: sign-in, or the shell of an account that
    // is already signed in.
    let back = if s.session.signed_in {
        "Not now"
    } else {
        "Back to sign in"
    };
    // Under the card, where the sign-in boards put the way to the other card.
    let foot = row()
        .no_window_drag()
        .child(foot_link("link-back", back, State::close_link, look));
    screen(
        "link",
        col()
            .items_center()
            .gap(t::SPACE_24)
            .child(card)
            .child(foot),
        look,
    )
}

/// A card's screen on the app ground: the content centred while the window has room for
/// it, and scrolling from its top once it does not (a card that has grown a notice and a
/// second field, in a short window). Sign-in is one too.
pub(super) fn screen(key: &'static str, content: El, look: Look) -> El {
    // Room shared above and below while there is any; none of it once the content is
    // taller than the window, where plain centring would cut both ends off.
    let room = || el().w_full().grow(1.0);
    col()
        .key(key)
        .w_full()
        .h_full()
        .items_center()
        .fill(look.c.bg)
        .window_drag()
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(room())
        .child(col().items_center().py(t::SPACE_40).child(content))
        .child(room())
        .with_scroll_bar(look)
}

fn claim(s: &mut State, _: &mut Tx) {
    s.claim_link();
}

/// The request, to read or select: base64 with nowhere to break, so it wraps anywhere.
fn request(request: &str, look: Look) -> El {
    let c = look.c;
    col()
        .key("link-request")
        .w_full()
        .h(t::SPACE_24 * 4.0)
        .padding(t::SPACE_12)
        .radius(t::RADIUS_12)
        .fill(c.field)
        .border(1.0, c.border_form)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .role("group")
        .label("Link request")
        .child(
            text(request)
                .font(t::TEXT_META.mono())
                .color(c.ink_field)
                .selectable(true)
                .selection_color(c.accent_soft)
                .wrap_anywhere()
                .w_full(),
        )
}
