//! Sign in: the instance with its trust card (architecture 6.1), then a password with a
//! second factor when the account asks for one, or a new account. The same flow and wording
//! as the web's sign-in screen and the iOS app's `SignInView.swift`. With them, when the instance offers
//! each: a sign-in link by mail, completed by its code ([`magic_link`]), and the way to
//! linking this computer to an identity ([`link_identity`]).
//!
//! A passkey and the instance's OIDC provider sign in through the system browser
//! (architecture 6.3, [`browser_sign_in`]).
//!
//! Drawn as the sign-in boards draw it: the stacked logo, the card, the line under it. Under
//! the primary button an "or" rule, then every other way in as an outlined button with its
//! glyph. Of what the boards show, the shell has no "Keep me signed in", no "Forgot
//! password?" and no Help, Privacy and Terms links, so none is drawn; the trust card and the instance field are the shell's own, in the boards' terms.

use lumia_client::app::InstanceSummary;
use rune::{Cursor, El, Tx, col, el, icon, row, text};

use super::components::{
    Look, card, error_text, field, field_with, link_button, notice, primary_button, reveal_button,
    wide_button,
};
use crate::app::State;
use crate::tokens::{self as t, icons};

pub fn view(s: &State, look: Look) -> El {
    let (body, foot) = match &s.session.info {
        Some(info) if s.session.is_open() => {
            let (card, foot) = account_card(s, info, look);
            (card, Some(foot))
        }
        _ => (instance_card(s, look), None),
    };
    // Centred while the window has room, scrolling once the card has outgrown it (the
    // account card with every way in the instance offers).
    super::link::screen(
        "sign-in",
        col()
            .items_center()
            .gap(t::SPACE_24)
            .child(logo(look))
            .child(body)
            .children(foot),
        look,
    )
}

/// The stacked logo above a card, in the file for the ground it is on.
pub(super) fn logo(look: Look) -> El {
    let art = if look.dark {
        &icons::LOGO_STACKED_DARK
    } else {
        &icons::LOGO_STACKED_LIGHT
    };
    el().w(icons::LOGO_STACKED_WIDTH)
        .h(icons::LOGO_STACKED_HEIGHT)
        .child(icon(art, icons::LOGO_STACKED_WIDTH).label("Lumia"))
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

/// Who runs the instance, before anything is sent to it: an inset block, as the boards draw
/// what sits inside a surface, with the way back at its end.
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
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_2)
        .child(
            text(info.host.clone())
                .font(t::TEXT_AUTHOR)
                .color(c.ink)
                .truncate(),
        )
        .child(
            text(format!("run by {operator}{version}"))
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .wrap(),
        );
    row()
        .key("trust")
        .w_full()
        .px(t::SPACE_12)
        .py(t::SPACE_10)
        .gap(t::SPACE_12)
        .items_center()
        .radius(t::RADIUS_12)
        .fill(c.inset)
        .child(words)
        .child(link_button(
            "change-instance",
            "Change instance",
            State::change_instance,
            look,
        ))
}

/// The rule between the primary button and the other ways in.
fn or_rule(look: Look) -> El {
    let c = look.c;
    let rule = || el().grow(1.0).h(1.0).fill(c.line_auth);
    row()
        .w_full()
        .gap(t::SPACE_12)
        .items_center()
        .a11y_hidden()
        .child(rule())
        .child(text("or").font(t::TEXT_SECTION).color(c.ink_muted))
        .child(rule())
}

/// The account card, and the line under it.
fn account_card(s: &State, info: &InstanceSummary, look: Look) -> (El, El) {
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
        .child(field_with(
            "password",
            "Password",
            &form.password,
            password_hint,
            Some(reveal_button(
                "show-password",
                form.password.is_revealed(),
                State::toggle_password,
                look,
            )),
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
    if !form.creating {
        // The other ways in, each an outlined button, a step apart under the rule.
        let ways = col()
            .w_full()
            .gap(t::SPACE_8)
            .children(browser_sign_in(s, info, look))
            .children(magic_link_ask(s, info, look))
            // While a mailed code is being waited for, this one follows that.
            .children(link_identity(s, info, look).filter(|_| !form.magic_sent));
        card = card.child(or_rule(look)).child(ways);
        card = magic_link_sent(card, s, info, look);
        if form.magic_sent {
            card = card.children(link_identity(s, info, look));
        }
    }

    let quiet = |words: &str| text(words).font(t::TEXT_FOOT).color(c.ink_secondary);
    let footer = row().center().gap(t::SPACE_4).no_window_drag();
    let footer = if form.creating {
        footer
            .child(quiet("Already have an account?"))
            .child(foot_link(
                "to-sign-in",
                "Sign in",
                |s| {
                    s.sign_in.creating = false;
                    s.sign_in.error.clear();
                },
                look,
            ))
    } else if info.open_registration {
        footer.child(quiet("New to Lumia?")).child(foot_link(
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
    (card, footer)
}

/// The link in the line under the card: the line's size, at the author's weight.
pub(super) fn foot_link(
    key: &'static str,
    label: &str,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    el().key(key)
        .role("button")
        .label(label)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .radius(t::RADIUS_5)
        .on_press(press)
        .child(text(label).font(t::TEXT_FOOT_LINK).color(c.link))
}

// ---- The magic link, and linking this computer ------------------------------------------

fn complete_magic_link(s: &mut State, _: &mut Tx) {
    s.complete_magic_link();
}

/// The magic link (architecture 6.3), when the instance mails them: "Email me a sign-in
/// link" for the address in the handle field. Inert while there is nothing to mail a link
/// to. Once asked, [`magic_link_sent`] takes its place.
fn magic_link_ask(s: &State, info: &InstanceSummary, look: Look) -> Option<El> {
    let form = &s.sign_in;
    if form.creating || !info.magic_link_enabled || form.magic_sent {
        return None;
    }
    let ready = !form.busy && !form.handle.text.trim().is_empty();
    Some(wide_button(
        "magic-link",
        Some(&icons::MAIL),
        "Email me a sign-in link",
        ready,
        |s, _| s.request_magic_link(),
        look,
    ))
}

/// After the link was asked for: where it went, the field for its code and "Sign in with
/// the code". A refusal is worded in the card's one error line.
fn magic_link_sent(card: El, s: &State, info: &InstanceSummary, look: Look) -> El {
    let form = &s.sign_in;
    if form.creating || !info.magic_link_enabled || !form.magic_sent {
        return card;
    }
    let address = form.handle.text.trim();
    card.child(notice(
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
    ))
}

/// "Link this computer to your identity", when the instance lets identity proof sign in:
/// the way to the link screen (identity spec 4.2).
fn link_identity(s: &State, info: &InstanceSummary, look: Look) -> Option<El> {
    if s.sign_in.creating || !info.identity_proof_sign_in_enabled {
        return None;
    }
    Some(wide_button(
        "to-link",
        Some(&icons::KEY),
        "Link this computer to your identity",
        true,
        |s, _| s.start_linking(),
        look,
    ))
}

/// Signing in through the system browser (architecture 6.3), for the account card: the
/// instance's provider when it names one, then a passkey on the instance's own page, as
/// the iOS app's `SignInView.swift` orders them. Nothing while a new account is being made. Once a
/// button has sent the browser off, a note says where the sign-in went.
fn browser_sign_in(s: &State, info: &InstanceSummary, look: Look) -> Option<El> {
    if s.sign_in.creating {
        return None;
    }
    let enabled = !s.sign_in.busy;
    let mut ways = col().key("browser-sign-in").w_full().gap(t::SPACE_8);
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
            notice(
                &icons::GLOBE,
                "Finish the sign-in in your browser. Lumia takes over once you are in.",
                look,
            )
            .key("browser-waiting"),
        );
    }
    Some(ways)
}

/// An outlined button with the key glyph, which sends the system browser to sign in: the
/// boards' "Continue with SSO".
fn browser_button(key: &'static str, label: &str, enabled: bool, oidc: bool, look: Look) -> El {
    wide_button(
        key,
        Some(&icons::KEY),
        label,
        enabled,
        move |s, tx| s.sign_in_through_browser(oidc, tx),
        look,
    )
}
