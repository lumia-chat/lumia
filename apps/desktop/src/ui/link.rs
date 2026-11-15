//! Link this computer as a device of an identity. Signed in to the account (from "Private
//! messages need this computer" on Home and Devices), by a prompt on the account's devices
//! that hold the identity (identity spec 4.2.1): this computer waits for one to answer,
//! then shows the six digits that one shows too, and is linked once it is approved there.
//! Under the waiting screen, "Link another way" is the copied request and the typed code
//! (4.2), for when the other device cannot reach the instance; from sign-in, with no
//! account signed in to hold a session, that is the only way. The same card and wording as
//! the iOS app's `LinkView.swift` and the web's `link.tsx`.
//!
//! The approving side is `ui::prompt`, and Devices' "Approve a device" for the other way.
//!
//! Not here: the request as a QR code (the other shells show its text too).

use rune::{El, LiveRegion, LoopSpec, ScrollSpec, Tx, col, el, row, text};

use super::components::{Look, card, error_text, field, primary_button, small_button, wide_button};
use super::sign_in::foot_link;
use crate::app::State;
use crate::app::account::Prompted;
use crate::tokens::{self as t};
use crate::ui::scroll::WithScrollBar;

pub fn view(s: &State, look: Look) -> El {
    let c = look.c;
    let form = &s.account.link;
    if s.session.signed_in && !form.another_way {
        return prompted(s, look);
    }
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
                "On a device that has your private messages, open Devices, choose Approve a device and paste this request. Then type the code it shows here.",
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

/// Linking by a prompt on the account's other devices: waiting for one to answer, the
/// digits to compare, or how it ended with the way to try again.
fn prompted(s: &State, look: Look) -> El {
    let form = &s.account.link;
    let mut stage = col()
        .key("link-stage")
        .w_full()
        .gap(t::SPACE_18)
        .live_region(LiveRegion::Polite);
    if let Prompted::Compare(shown) = &form.prompted {
        stage = stage.child(digits("link-digits", shown, look));
    }
    stage = stage.child(
        text(form.prompted.words())
            .font(t::TEXT_BODY)
            .color(look.c.ink_secondary)
            .wrap()
            .w_full(),
    );
    if matches!(form.prompted, Prompted::Starting | Prompted::Waiting) {
        stage = stage.child(waiting("Waiting for your other device", look));
    }
    let mut card = card(
        "Link this computer",
        "A device you already use approves this one.",
        look,
    )
    .key("link-card")
    .no_window_drag()
    .child(stage);
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    let mut actions = col().w_full().gap(t::SPACE_8);
    if matches!(
        form.prompted,
        Prompted::Idle | Prompted::Declined | Prompted::Expired
    ) {
        actions = actions.child(primary_button(
            "link-again",
            "Try again",
            true,
            State::start_prompt,
            look,
        ));
    }
    card = card.child(actions.child(wide_button(
        "link-cancel",
        None,
        "Cancel",
        true,
        |s, _| s.close_link(),
        look,
    )));
    let mut content = col().items_center().gap(t::SPACE_24).child(card);
    // The copied request and the typed code, for an other device that cannot reach the
    // instance: offered while nothing has answered.
    if matches!(form.prompted, Prompted::Starting | Prompted::Waiting) {
        content = content.child(row().no_window_drag().child(foot_link(
            "link-another-way",
            "Link another way",
            State::link_another_way,
            look,
        )));
    }
    screen("link", content, look)
}

/// The six digits both screens show, large, in the code's type: `482 913`.
pub(super) fn digits(key: &'static str, shown: &str, look: Look) -> El {
    let c = look.c;
    row()
        .key(key)
        .w_full()
        .py(t::SPACE_14)
        .center()
        .radius(t::RADIUS_12)
        .fill(c.inset)
        .role("text")
        .label(format!("Number {shown}"))
        .child(
            text(shown)
                .font(t::TEXT_LINK_DIGITS)
                .color(c.ink)
                .a11y_hidden(),
        )
}

/// A quiet sign that something is on its way: three dots fading in turn, holding still
/// under reduced motion.
pub(super) fn waiting(label: &str, look: Look) -> El {
    let dot = |n: u8| {
        el().size(t::SPACE_6, t::SPACE_6)
            .radius(t::SPACE_6 / 2.0)
            .fill(look.c.ink_quiet)
            .looping(LoopSpec::opacity(
                t::WAITING_PERIOD,
                f64::from(n) * t::WAITING_STEP,
                t::WAITING_LOW,
                1.0,
            ))
    };
    row()
        .key("waiting")
        .gap(t::SPACE_6)
        .items_center()
        .role("progressbar")
        .label(label)
        .child(dot(0))
        .child(dot(1))
        .child(dot(2))
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
        .with_scroll_bar(key, look)
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
