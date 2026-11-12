//! Profile: how people see this person. The account's own handle on this instance, with
//! its display name on a computer that does not hold the identity (`PATCH /account`,
//! architecture 6.2: any signed-in device may change them); the status they chose
//! (protocol 6.7: online, away
//! or do not disturb, with a custom line), the signed profile the identity carries
//! (architecture 6.4: display name, pronouns and bio, signed by this computer's device key
//! and sent to the instance as the account's), and, for an account with no identity on
//! this computer, the way to linking it. The same states and wording as the web's
//! `home.tsx`.
//!
//! Sign out is at the foot of this page, and in the Account menu with "Change instance"
//! (`app::menu`); the page keeps it because a menu bar is macOS only.
//!
//! The account card also has the photo: a file chosen through the system's dialog becomes
//! the instance's own square of it (architecture 12, "Avatars"). The signed profile does
//! not name that picture yet (a save keeps whatever avatar the held profile names), so it
//! is this instance's alone and does not travel with the identity.

use rune::{El, Tx, col, row, text};

use super::devices::{action, link_notice, page, page_column};
use crate::app::State;
use crate::tokens::{self as t, icons};
use crate::ui::components::{
    Look, avatar_of, card, error_text, field, notice, primary_button, segmented, small_button,
};

pub fn view(s: &State, look: Look) -> El {
    let mut content = page_column()
        .child(account_card(s, look))
        .child(status(s, look));
    let form = &s.account.profile;
    content = if form.loaded || s.session.has_identity() {
        content.child(profile(s, look))
    } else {
        content.child(link_notice(
            "profile-link",
            "Your profile travels with your identity, signed by its devices. This computer holds no device key yet; a device you already use approves it.",
            look,
        ))
    };
    content = content.child(account(s, look));
    page("profile", "Profile", "How people see you", content, look)
}

fn quiet(words: &str, look: Look) -> El {
    text(words)
        .font(t::TEXT_META)
        .color(look.c.ink_secondary)
        .wrap()
        .w_full()
}

fn save_status(s: &mut State, _: &mut Tx) {
    s.save_status();
}

fn save_profile(s: &mut State, _: &mut Tx) {
    s.save_profile();
}

fn save_account(s: &mut State, _: &mut Tx) {
    s.save_account();
}

/// The account's photo: what it is now, a way to choose another, and a way to have none.
/// The instance makes its own square of the file (architecture 12), so nothing of the
/// file is passed on but what it shows.
fn photo(s: &State, look: Look) -> El {
    let form = &s.account.profile.photo;
    let has = !s.session.avatar_url_of(s.session.me()).is_empty();
    let mut buttons = row().gap(t::SPACE_8).items_center().child(small_button(
        "choose-photo",
        if form.busy {
            "Sending…"
        } else if has {
            "Change photo…"
        } else {
            "Choose a photo…"
        },
        !form.busy,
        |s, tx| s.choose_photo(tx),
        look,
    ));
    if has {
        buttons = buttons.child(small_button(
            "remove-photo",
            "Remove",
            !form.busy,
            |s, _| s.remove_photo(),
            look,
        ));
    }
    let mut words = col().grow(1.0).min_w(0.0).gap(t::SPACE_6).child(buttons);
    words = words.child(if !form.error.is_empty() {
        error_text(&form.error, look)
    } else if form.saved {
        quiet("Saved.", look)
    } else {
        quiet(
            "A PNG, JPEG, WebP or GIF up to 8 MB. People see its middle, as a circle.",
            look,
        )
    });
    row()
        .key("photo")
        .w_full()
        .gap(t::SPACE_14)
        .items_center()
        .child(avatar_of(
            s,
            s.session.self_name(),
            s.session.me(),
            t::AVATAR_CARD,
            look,
        ))
        .child(words)
}

/// The account on this instance: the handle people find and mention it by, and the name
/// they see where no signed profile gives one from this computer.
fn account_card(s: &State, look: Look) -> El {
    let form = &s.account.profile.account;
    let mut card = card(
        "Account",
        &format!("Who you are on {}.", s.session.address()),
        look,
    )
    .key("account-form")
    .w_full()
    .child(photo(s, look))
    .child(field(
        "handle",
        "Handle",
        &form.handle,
        "ada",
        |s| &mut s.account.profile.account.handle,
        save_account,
        look,
    ));
    if s.names_account() {
        card = card.child(field(
            "account-name",
            "Display name",
            &form.display_name,
            "How people see you",
            |s| &mut s.account.profile.account.display_name,
            save_account,
            look,
        ));
    }
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    if form.saved {
        card = card.child(quiet("Saved.", look));
    }
    card.child(action(primary_button(
        "save-account",
        "Save",
        !form.busy && !form.handle.text.trim().is_empty(),
        State::save_account,
        look,
    )))
}

/// What people see beside this person's name in a server's member list.
fn status(s: &State, look: Look) -> El {
    let form = &s.account.profile;
    let chosen = match form.status.as_str() {
        "AWAY" => "AWAY",
        "DO_NOT_DISTURB" => "DO_NOT_DISTURB",
        _ => "ONLINE",
    };
    let mut card = card(
        "Status",
        "What people see beside your name in a server's member list.",
        look,
    )
    .key("status-card")
    .w_full()
    .child(segmented(
        "status",
        "Status",
        &[
            ("ONLINE", "Online"),
            ("AWAY", "Away"),
            ("DO_NOT_DISTURB", "Do not disturb"),
        ],
        chosen,
        |s: &mut State, status: &str| s.pick_status(status),
        look,
    ))
    .child(field(
        "custom-status",
        "Custom status",
        &form.custom_status,
        "At lunch",
        |s| &mut s.account.profile.custom_status,
        save_status,
        look,
    ));
    if !form.status_error.is_empty() {
        card = card.child(error_text(&form.status_error, look));
    }
    if form.status_saved {
        card = card.child(quiet("Status saved.", look));
    }
    card.child(action(primary_button(
        "save-status",
        "Save status",
        true,
        State::save_status,
        look,
    )))
}

/// The signed profile's form. It waits for the profile held: fields shown before it lands
/// would be overwritten by it, typing and all.
fn profile(s: &State, look: Look) -> El {
    let form = &s.account.profile;
    let title = match s.session.snapshot.as_ref().map(|snap| snap.handle.as_str()) {
        Some(handle) if !handle.is_empty() => format!("@{handle}"),
        _ => "You".into(),
    };
    let subtitle = match (&s.session.identity.profile, form.loaded) {
        (_, false) => "Looking…".to_string(),
        (Some(held), true) => format!(
            "Version {}, signed by this computer's device key; every instance you sign in to takes it.",
            held.version
        ),
        (None, true) => "Nothing set yet: people see your handle.".to_string(),
    };
    let mut card = card(&title, &subtitle, look).key("profile-card").w_full();
    if !form.loaded {
        return card;
    }
    card = card
        .child(field(
            "display-name",
            "Display name",
            &form.display_name,
            "How people see you",
            |s| &mut s.account.profile.display_name,
            save_profile,
            look,
        ))
        .child(field(
            "pronouns",
            "Pronouns",
            &form.pronouns,
            "they/them",
            |s| &mut s.account.profile.pronouns,
            save_profile,
            look,
        ))
        .child(field(
            "bio",
            "Bio",
            &form.bio,
            "A line about you",
            |s| &mut s.account.profile.bio,
            save_profile,
            look,
        ));
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    if form.saved {
        card = card.child(quiet("Saved. This instance shows it now.", look));
    }
    card.child(action(primary_button(
        "save-profile",
        "Save",
        !form.busy,
        State::save_profile,
        look,
    )))
}

/// Who is signed in and where, with the way out.
fn account(s: &State, look: Look) -> El {
    notice(
        &icons::KEY,
        format!(
            "Signed in to {} as {}.",
            s.session.address(),
            s.session.self_name()
        ),
        look,
    )
    .key("account")
    .items_center()
    .child(small_button(
        "sign-out",
        "Sign out",
        true,
        |s, _| s.sign_out(),
        look,
    ))
}
