//! Sign in: the instance with its trust card (architecture 6.1), then a password with a
//! second factor when the account asks for one, or a new account. The same flow and wording
//! as the web's sign-in screen and `SignInView.swift`.
//!
//! Not here yet: the magic link, the browser handoff for passkeys and OIDC (architecture
//! 6.3), and linking this computer to an identity.

use lumia_client::app::InstanceSummary;
use rune::{El, Tx, col, icon, row, text};

use super::components::{Look, card, error_text, field, link_button, primary_button};
use crate::app::State;
use crate::tokens::{self as t, icons};

pub fn view(s: &State, look: Look) -> El {
    let c = look.c;
    let body = match &s.session.info {
        Some(info) if s.session.is_open() => account_card(s, info, look),
        _ => instance_card(s, look),
    };
    col()
        .key("sign-in")
        .w_full()
        .h_full()
        .center()
        .gap(t::SPACE_24)
        .fill(c.bg)
        .window_drag()
        .child(icon(&icons::LOGO, t::TILE_SIZE).label("Lumia"))
        .child(body)
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
