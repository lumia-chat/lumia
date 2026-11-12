//! Sign in: the instance with its trust card (architecture 6.1), then a password with a
//! second factor when the account asks for one, or a new account. The same flow and wording
//! as the web's sign-in screen and `SignInView.swift`. With them, when the instance offers
//! each: a sign-in link by mail, completed by its code ([`magic_link`]), and the way to
//! linking this computer to an identity ([`link_identity`]).
//!
//! A passkey and the instance's OIDC provider sign in through the system browser
//! (architecture 6.3, [`browser_sign_in`]).

use lumia_client::app::InstanceSummary;
use rune::{El, Tx, col, icon, row, text};

use super::components::{Look, card, error_text, field, link_button, primary_button};
use crate::app::State;
use crate::tokens::{self as t, icons};

pub fn view(s: &State, look: Look) -> El {
    let body = match &s.session.info {
        Some(info) if s.session.is_open() => account_card(s, info, look),
        _ => instance_card(s, look),
    };
    // Centred while the window has room, scrolling once the card has outgrown it (the
    // account card with every way in the instance offers).
    super::link::screen(
        "sign-in",
        col()
            .items_center()
            .gap(t::SPACE_24)
            .child(icon(&icons::LOGO, t::TILE_SIZE).label("Lumia"))
            .child(body),
        look,
    )
}

fn connect(s: &mut State, _: &mut Tx) {
    s.connect();
}

fn submit(s: &mut State, _: &mut Tx) {
    s.submit();
}

fn instance_card(s: &State, look: Look) -> El {
    let form = &s.sign_in;
    let mut card = card("Welcome", "Which instance do you want to sign in to?", look)
        .key("instance-card")
        .no_window_drag()
        .child(field(
            "instance",
            "Instance",
            &form.instance,
            "https://chat.example.com",
            |s| &mut s.sign_in.instance,
            connect,
            look,
        ));
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    if form.reaching {
        card = card.child(
            text("Reaching the instance…")
                .font(t::TEXT_META)
                .color(look.c.ink_secondary),
        );
    }
    card.child(primary_button(
        "continue",
        "Continue",
        !form.reaching,
        State::connect,
        look,
    ))
}

/// Who runs the instance, before anything is sent to it.
fn trust(info: &InstanceSummary, look: Look) -> El {
    let c = look.c;
    let operator = if info.operator_name.is_empty() {
        "an unnamed operator"
    } else {
        info.operator_name.as_str()
    };
    let version = if info.software_version.is_empty() {
        String::new()
    } else {
        format!(" · Lumia {}", info.software_version)
    };
    col()
        .key("trust")
        .w_full()
        .padding(t::SPACE_10)
        .gap(t::SPACE_2)
        .radius(t::RADIUS_8)
        .fill(c.inset)
        .child(
            text(info.host.clone())
                .font(t::TEXT_AUTHOR)
                .color(c.ink_emphasis),
        )
        .child(
            text(format!("run by {operator}{version}"))
                .font(t::TEXT_META)
                .color(c.ink_secondary)
                .wrap(),
        )
        .child(link_button(
            "change-instance",
            "Change instance",
            State::change_instance,
            look,
        ))
}

fn account_card(s: &State, info: &InstanceSummary, look: Look) -> El {
    let c = look.c;
    let form = &s.sign_in;
    let (title, subtitle) = if form.creating {
        (
            "Create an account",
            format!("A new account on {}.", info.host),
        )
    } else {
        (
            "Welcome back",
            "Sign in to pick up your conversations.".to_string(),
        )
    };
    let (handle_label, handle_hint, password_hint, action) = if form.creating {
        (
            "Handle",
            "yourname",
            "At least 12 characters",
            "Create account",
        )
    } else {
        (
            "Email or handle",
            "you@example.com",
            "Your password",
            "Sign in",
        )
    };
    let mut card = card(title, &subtitle, look)
        .key("account-card")
        .no_window_drag()
        .child(trust(info, look))
        .child(field(
            "handle",
            handle_label,
            &form.handle,
            handle_hint,
            |s| &mut s.sign_in.handle,
            submit,
            look,
        ))
        .child(field(
            "password",
            "Password",
            &form.password,
            password_hint,
            |s| &mut s.sign_in.password,
            submit,
            look,
        ));
    if form.needs_totp {
        card = card.child(field(
            "totp",
            "Verification code",
            &form.totp,
            "6 digits from your authenticator",
            |s| &mut s.sign_in.totp,
            submit,
            look,
        ));
    }
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    card = card.child(primary_button(
        "submit",
        action,
        !form.busy,
        State::submit,
        look,
    ));
    card = card.children(browser_sign_in(s, info, look));
    card = magic_link(card, s, info, look);
    card = link_identity(card, s, info, look);

    let quiet = |words: &str| text(words).font(t::TEXT_META).color(c.ink_secondary);
    let footer = row().w_full().center().gap(t::SPACE_4);
    let footer = if form.creating {
        footer
            .child(quiet("Already have an account?"))
            .child(link_button(
                "to-sign-in",
                "Sign in",
                |s| {
                    s.sign_in.creating = false;
                    s.sign_in.error.clear();
                },
                look,
            ))
    } else if info.open_registration {
        footer.child(quiet("New to Lumia?")).child(link_button(
            "to-create",
            "Create an account",
            |s| {
                s.sign_in.creating = true;
                s.sign_in.needs_totp = false;
                s.sign_in.error.clear();
            },
            look,
        ))
    } else {
        footer.child(quiet(
            "New to Lumia? This instance is not open for new accounts.",
        ))
    };
    card.child(footer)
}

// ---- The magic link, and linking this computer ------------------------------------------

fn complete_magic_link(s: &mut State, _: &mut Tx) {
    s.complete_magic_link();
}

/// The magic link (architecture 6.3), when the instance mails them: "Email me a sign-in
/// link" for the address in the handle field, then where it went, the field for its code
/// and "Sign in with the code". A refusal is worded in the card's one error line.
fn magic_link(card: El, s: &State, info: &InstanceSummary, look: Look) -> El {
    let c = look.c;
    let form = &s.sign_in;
    if form.creating || !info.magic_link_enabled {
        return card;
    }
    let address = form.handle.text.trim();
    if form.magic_sent {
        return card
            .child(super::components::notice(
                &icons::MAIL,
                format!(
                    "A link is on its way to {address} if it has an account here. Open it on this device, or paste its code below."
                ),
                look,
            ))
            .child(field(
                "magic-code",
                "Code from the email",
                &form.magic_code,
                "Paste the code",
                |s| &mut s.sign_in.magic_code,
                complete_magic_link,
                look,
            ))
            .child(primary_button(
                "magic-sign-in",
                "Sign in with the code",
                !form.busy && !form.magic_code.text.trim().is_empty(),
                State::complete_magic_link,
                look,
            ));
    }
    const LABEL: &str = "Email me a sign-in link";
    let ask = if form.busy || address.is_empty() {
        // Nothing to mail a link to yet: the link is there, and inert.
        rune::el()
            .key("magic-link")
            .role("button")
            .label(LABEL)
            .disabled(true)
            .opacity(0.5)
            .child(text(LABEL).font(t::TEXT_LINK_SM).color(c.link))
    } else {
        link_button("magic-link", LABEL, State::request_magic_link, look)
    };
    card.child(col().w_full().items_center().child(ask))
}

/// "Link this computer to your identity", when the instance lets identity proof sign in:
/// the way to the link screen (identity spec 4.2).
fn link_identity(card: El, s: &State, info: &InstanceSummary, look: Look) -> El {
    let c = look.c;
    if s.sign_in.creating || !info.identity_proof_sign_in_enabled {
        return card;
    }
    const LABEL: &str = "Link this computer to your identity";
    card.child(
        row()
            .key("to-link")
            .w_full()
            .h(t::CONTROL_HEIGHT - t::SPACE_8)
            .center()
            .gap(t::SPACE_8)
            .radius(t::RADIUS_10)
            .fill(c.surface)
            .border(1.0, c.border_form)
            .hover(c.hover_secondary)
            .role("button")
            .label(LABEL)
            .cursor(rune::Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(State::start_linking)
            .child(icon(&icons::KEY, t::SPACE_16).color(c.ink_field))
            .child(
                text(LABEL)
                    .font(t::TEXT_BUTTON_SECONDARY)
                    .color(c.ink_field),
            ),
    )
}

/// Signing in through the system browser (architecture 6.3), for the account card: the
/// instance's provider when it names one, then a passkey on the instance's own page, as
/// `SignInView.swift` orders them. Nothing while a new account is being made. Once a
/// button has sent the browser off, a note says where the sign-in went.
fn browser_sign_in(s: &State, info: &InstanceSummary, look: Look) -> Option<El> {
    if s.sign_in.creating {
        return None;
    }
    let enabled = !s.sign_in.busy;
    let mut ways = col().key("browser-sign-in").w_full().gap(t::SPACE_12);
    if !info.oidc_name.is_empty() {
        ways = ways.child(browser_button(
            "oidc-sign-in",
            &info.oidc_name,
            enabled,
            true,
            look,
        ));
    }
    ways = ways.child(browser_button(
        "passkey-sign-in",
        "Sign in with a passkey",
        enabled,
        false,
        look,
    ));
    if s.links.waiting(&s.session.base_url) {
        ways = ways.child(
            super::components::notice(
                &icons::GLOBE,
                "Finish the sign-in in your browser. Lumia takes over once you are in.",
                look,
            )
            .key("browser-waiting"),
        );
    }
    Some(ways)
}

/// A full-width outlined button with the key glyph, which sends the system browser to
/// sign in: the design's secondary button, as wide as the card.
fn browser_button(key: &'static str, label: &str, enabled: bool, oidc: bool, look: Look) -> El {
    let c = look.c;
    let button = row()
        .key(key)
        .w_full()
        .h(t::CONTROL_HEIGHT - t::SPACE_8)
        .px(t::SPACE_14)
        .gap(t::SPACE_8)
        .center()
        .radius(t::RADIUS_10)
        .fill(c.surface)
        .border(1.0, c.border_form)
        .role("button")
        .label(label)
        .disabled(!enabled)
        .child(icon(&icons::KEY, t::SPACE_16).color(c.ink_quiet))
        .child(
            text(label)
                .font(t::TEXT_BUTTON_SECONDARY)
                .color(c.ink_field),
        );
    if enabled {
        button
            .hover(c.hover_secondary)
            .cursor(rune::Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press_tx(move |s: &mut State, tx| s.sign_in_through_browser(oidc, tx))
    } else {
        button.opacity(0.5)
    }
}
